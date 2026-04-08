/// Behavior tree — enum-based, declarative, zero closures.
///
/// All bot behavior (encounters, world, reactive combat) is expressed as a tree
/// of `Bt` variants. Built once at init, ticked every update. Pure data — no
/// `Box<dyn>` closures, no vtable dispatch (except through `TickContext`).
///
/// # Design
///
/// Leaf nodes are either **conditions** (return Success/Failure) or **actions**
/// (issue commands via `ctx.interface`, return Success/Failure/Running).
/// Composition uses `Seq`, `Sel`, `Not`, and `Throttle`.
///
/// # Example
/// ```ignore
/// use Bt::*;
///
/// Sel(vec![
///     Seq(vec![Bt::self_has(LIVING_BOMB), MoveAwayFromRaid(40.0)]),
///     Seq(vec![Not(Box::new(InCombat)), Follow]),
/// ])
/// ```
use crate::bot::formation::{
    ChaosState, FormationContext, FormationMember, FormationOutput, resolve_formation,
};
use crate::bot::settings::{BehaviorMode, Reactivity, StrategyFlags};
use crate::bot::state::PlayerClass;
use crate::engine::blackboard::{Key as BbKey, Value as BbValue};
use crate::engine::bt_nodes::{BtNode, BtResult};
use crate::engine::context::TickContext;
use crate::engine::throttles::ThrottleKey;
use crate::ffi::{ItemId, SpellId, UnitHandle};
use crate::noncombat::buffing::GroupBuff;

/// Terse constructor for [`Bt::Seq`]. Accepts a comma-separated list of
/// child nodes (trailing comma allowed). PascalCase matches the variant
/// name for readability — Rust allows this since macros live in their own
/// namespace and there is no snake_case lint on macro identifiers.
///
/// ```ignore
/// Seq!(Bt::self_missing(SLICE_AND_DICE), CastOnSelf(SLICE_AND_DICE))
/// // expands to
/// Bt::Seq(vec![Bt::self_missing(SLICE_AND_DICE), CastOnSelf(SLICE_AND_DICE)])
/// ```
#[macro_export]
#[allow(non_snake_case)]
macro_rules! Seq {
    ($($x:expr),* $(,)?) => {
        $crate::engine::bt::Bt::Seq(vec![$($x),*])
    };
}

/// Terse constructor for [`Bt::Sel`]. See [`Seq!`] for rationale.
#[macro_export]
#[allow(non_snake_case)]
macro_rules! Sel {
    ($($x:expr),* $(,)?) => {
        $crate::engine::bt::Bt::Sel(vec![$($x),*])
    };
}

/// A numeric resource the BT can read for comparison. Covers self and target
/// health, all primary power types (mana / rage / energy / runic power),
/// and rogue/feral combo points. Used by [`Bt::Cmp`].
///
/// Values are always `u32`. Percentage variants are integer 0–100 (not
/// 0.0–1.0) — this keeps the whole comparison pipeline on one type and is
/// plenty precise for threshold gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    /// Bot's current HP.
    SelfHealth,
    /// Bot's HP as integer percent (0..=100).
    SelfHealthPct,
    /// Bot's current mana (only meaningful when primary power is mana; reads
    /// as 0 otherwise so comparisons still behave consistently).
    SelfMana,
    /// Bot's mana as integer percent (0..=100). Reads 0 when the bot does
    /// not use mana as primary power.
    SelfManaPct,
    /// Bot's rage (0 unless primary power is rage).
    SelfRage,
    /// Bot's energy (0 unless primary power is energy).
    SelfEnergy,
    /// Bot's runic power (0 unless primary power is runic power, WotLK DK).
    SelfRunicPower,
    /// Rogue/feral combo points on the current target.
    SelfComboPoints,
    /// Current target's HP.
    TargetHealth,
    /// Current target's HP as integer percent (0..=100). Reads 0 when there
    /// is no target.
    TargetHealthPct,
    /// Distance in yards from the bot to its current target (truncated to
    /// `u32`). Reads `u32::MAX` when there is no target, so `Below(n)` safely
    /// fails and `Above(n)` safely succeeds.
    TargetDistance,
    /// Number of hostile units currently in the bot's nearby scan.
    NearbyCount,
    /// Number of attackers currently targeting the bot.
    AttackerCount,
    /// Current group/raid size including self (0 when solo).
    GroupSize,
}

/// Comparison operator for [`Bt::Cmp`].
///
/// `Above` / `Below` are strict `>` / `<`; `AtLeast` / `AtMost` are inclusive
/// `>=` / `<=`; `Exactly` is `==`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Above(u32),
    Below(u32),
    Exactly(u32),
    AtLeast(u32),
    AtMost(u32),
}

/// Weapon categories matching CMaNGOS `ITEM_SUBCLASS_WEAPON_*`. Used by the
/// `MainHandIs` / `OffHandIs` / `RangedIs` BT nodes to gate class abilities
/// that require a specific weapon (e.g. rogue Backstab requires Dagger).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponType {
    Axe1H = 0,
    Axe2H = 1,
    Bow = 2,
    Gun = 3,
    Mace1H = 4,
    Mace2H = 5,
    Polearm = 6,
    Sword1H = 7,
    Sword2H = 8,
    Staff = 10,
    Fist = 13,
    Dagger = 15,
    Thrown = 16,
    Crossbow = 18,
    Wand = 19,
    FishingPole = 20,
}

impl WeaponType {
    #[inline]
    pub const fn subclass(self) -> u32 {
        self as u32
    }
}

/// WoW reputation rank (0..=7). Used by [`Bt::RepWithFactionBelow`]. The
/// discriminant values match the server's `ReputationRank` enum — `Neutral`
/// (3) is the fallback the C++ FFI returns for factions the bot has never
/// touched, which mirrors how the WoW client displays unfilled entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ReputationRank {
    Hated = 0,
    Hostile = 1,
    Unfriendly = 2,
    Neutral = 3,
    Friendly = 4,
    Honored = 5,
    Revered = 6,
    Exalted = 7,
}

impl ReputationRank {
    /// Raw tier byte for comparison against `BotInterface::reputation_rank`.
    #[inline]
    pub const fn raw(self) -> u8 {
        self as u8
    }
}

/// Which dispel school the [`Bt::PartyMemberNeedsDispel`] condition should
/// match on. `Any` is PB2's `DispelTrigger` default — the bot takes whatever
/// it can clean regardless of school. The individual variants map 1:1 to
/// server-side `DispelType` values and are encoded as a single-bit mask on
/// the C FFI side (bit positions match `1 << DispelType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispelSchool {
    /// Any school the bot can currently dispel.
    Any,
    /// Arcane / magic debuffs (Dispel Magic / Cleanse target set).
    Magic,
    /// Curses (Remove Curse).
    Curse,
    /// Diseases (Cure Disease / Cleanse / Abolish Disease).
    Disease,
    /// Poisons (Cure Poison / Cleanse / Abolish Poison).
    Poison,
}

impl DispelSchool {
    /// Encode as the `dispel_mask` byte expected by `BotInterface::
    /// find_dispellable_target`. `Any` collapses to `0`, which the C++
    /// side interprets as "no restriction". Other variants use
    /// `1 << DispelType` (magic=1, curse=2, disease=3, poison=4 on the
    /// server), giving bits 2/4/8/16.
    #[inline]
    pub const fn mask(self) -> u8 {
        match self {
            Self::Any => 0,
            Self::Magic => 1 << 1,
            Self::Curse => 1 << 2,
            Self::Disease => 1 << 3,
            Self::Poison => 1 << 4,
        }
    }
}

/// Which unit an [`Bt::Aura`] check applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuraUnit {
    /// The bot itself.
    Self_,
    /// The bot's current target.
    Target,
}

/// How an [`Bt::Aura`] check identifies the aura to look for.
#[derive(Debug, Clone, Copy)]
pub enum AuraKey {
    /// Single specific spell id.
    Spell(SpellId),
    /// Any rank from a static rank list — matches any entry in the slice.
    /// Used for multi-rank DoTs/buffs (Rend, Renew, Moonfire, etc.).
    AnyRank(&'static [SpellId]),
}

/// What to check about the aura's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuraPred {
    /// Aura is present on the unit.
    Present,
    /// Aura is missing from the unit.
    Missing,
    /// Aura has strictly fewer than `n` stacks (missing counts as 0 stacks).
    StacksBelow(u8),
}

/// Boolean settings that can be checked by `SettingEnabled`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    AutoMount,
    AutoLoot,
    AutoVendor,
    AutoRepair,
    AutoAcceptQuest,
    AutoResurrect,
    Verbose,
}

#[derive(Debug, Clone)]
pub enum Bt {
    // ── Compositors ──────────────────────────────────────────────────────
    /// All children must succeed in order. Fails on first failure.
    Seq(Vec<Bt>),
    /// First child that succeeds wins. Fails if all children fail.
    Sel(Vec<Bt>),
    /// Inverts Success ↔ Failure (Running unchanged).
    Not(Box<Bt>),
    /// Run child but always return Success. Used to make a subtree
    /// non-blocking in a Seq — the child's result (including Running)
    /// is discarded so the Seq continues to the next step.
    Optional(Box<Bt>),
    /// Run child at most once per `interval_ms`. Returns Failure when throttled.
    ///
    /// The tree itself is stateless — last-fire timestamps live on the bot in
    /// [`crate::engine::throttles::Throttles`], keyed by the construction
    /// site's `(file, line)` via [`Bt::throttle`]'s `#[track_caller]` capture.
    /// This lets the same `Bt` be safely shared between bots or returned as
    /// `&'static Bt` from encounter `phase_bt()`.
    Throttle {
        key: ThrottleKey,
        interval_ms: u64,
        child: Box<Bt>,
    },

    // ── Conditions — encounter ───────────────────────────────────────────
    /// Unified aura state check. Use the `Bt::self_missing` / `self_has`
    /// / `target_has` / `target_missing` / `self_missing_any_rank` /
    /// `target_missing_any_rank` / `target_aura_stacks_below` helper
    /// constructors on `Bt` for ergonomic call-site construction — this
    /// struct variant is the underlying data model.
    Aura {
        unit: AuraUnit,
        key: AuraKey,
        pred: AuraPred,
    },
    /// Bot has learned `spell_id` (`Player::HasSpell`). Use to gate
    /// talented or optional abilities like Hemorrhage, Stormstrike, or
    /// Metamorphosis before attempting to cast them.
    KnowsSpell(SpellId),
    /// Current target is casting any spell (not necessarily interruptible).
    TargetIsCasting,
    /// This bot is the specified class.
    IsClass(PlayerClass),
    /// This bot has the TANK role.
    IsTank,
    /// This bot is ranged DPS or healer.
    IsRanged,
    /// This bot is melee DPS (not tank, not ranged).
    IsMeleeDps,
    /// At least `count` group members (including self) are below `threshold` HP fraction.
    GroupMembersBelow(u8, f32),

    // ── Conditions — general ─────────────────────────────────────────────
    /// No-op: always succeeds, consumes the tick.
    Noop,
    /// Bot is currently in combat.
    InCombat,
    /// Bot is alive (not dead/ghost).
    IsAlive,
    /// Bot is a ghost (released spirit, corpse running).
    IsGhost,
    /// Bot is on a taxi flight path.
    OnTaxi,
    /// Bot is mounted.
    IsMounted,
    /// Bot is indoors.
    IsIndoor,
    /// Bot is currently moving.
    IsMoving,
    /// Bot's current behavior mode matches.
    ModeIs(BehaviorMode),
    /// Bot's strategy flags contain all of these. Use to gate opt-in
    /// subtrees (RPG branches, RTSC, grind extras, CC management,
    /// combat-order targeting like tank/assist/protect, boost, etc.).
    StrategyEnabled(StrategyFlags),
    /// Bot's reactivity level matches.
    ReactivityIs(Reactivity),
    /// Bot uses mana (not rage/energy/runic power).
    UsesMana,
    /// Compare a numeric resource (HP / mana / rage / energy / combo points /
    /// target HP) against a threshold. Unified replacement for the
    /// proliferating per-resource variants. Reads cleanly as
    /// `Cmp(SelfEnergy, Above(60))`.
    Cmp(Resource, Op),
    /// Bot is positioned in the current target's rear arc
    /// (Backstab / Shred requirement).
    IsBehindTarget,
    /// Bot's main-hand weapon is of the given type.
    MainHandIs(WeaponType),
    /// Bot's off-hand weapon is of the given type.
    OffHandIs(WeaponType),
    /// Bot's ranged slot holds a weapon of the given type.
    RangedIs(WeaponType),
    /// Bot is in the given shapeshift form / warrior stance
    /// (`ShapeshiftForm` enum id — 0=none, 1=cat, 5=bear, 17=battle stance,
    /// 18=defensive stance, 19=berserker stance, 31=moonkin, …).
    InShapeshift(u8),
    /// Bot has at least `n` of `ITEM_ID_SOUL_SHARD` (6265) in inventory.
    /// Warlock shard-cost gate (Summon Imp, Healthstone, Soulstone).
    SoulShardsAtLeast(u8),
    /// Shaman has an active totem in `slot` (0=fire, 1=earth, 2=water, 3=air).
    HasTotem(u8),
    /// Main-hand has a temporary enchant applied (shaman weapon buff,
    /// rogue poison, warrior sharpening stone).
    MainHandEnchanted,
    /// Off-hand has a temporary enchant applied.
    OffHandEnchanted,
    /// At least `n` death-knight rune slots are ready to use (WotLK only).
    /// Always fails on Classic/TBC.
    RunesReady(u8),
    /// A boolean setting is enabled.
    SettingEnabled(Setting),
    /// Bot has a focus target set.
    HasFocusTarget,
    /// Bot has a protect target set.
    HasProtectTarget,
    /// There are units attacking this bot.
    HasAttackers,
    /// There are hostile units nearby.
    HasNearby,
    /// Blackboard has a travel destination set.
    HasTravelDest,
    /// Bot has items to sell.
    HasSellableItems,
    /// Equipment durability is below this fraction.
    DurabilityBelow(f32),
    /// Current target is casting an interruptible spell.
    TargetCastingInterruptible,
    /// Bot is about to pull aggro (threat > 90% of tank).
    PullingAggro,
    /// Bot is Hunter or Warlock (pet classes).
    IsPetClass,
    /// Bot has a pet (alive or dead).
    HasPet,
    /// Bot's pet is alive.
    PetAlive,
    /// Bot's pet happiness is below content (< 2).
    PetUnhappy,
    /// Bot should proactively engage based on reactivity + attackers/nearby.
    ShouldEngage,
    /// Bot should flee: either an explicit flee command is active
    /// (`BotSettings::flee_override_until_ms` not yet expired), or health
    /// has dropped below `BotSettings::flee_hp_pct` while `StrategyFlags::FLEE`
    /// is enabled. Reads live settings so threshold changes take effect
    /// next tick.
    ShouldFlee,

    // ── Conditions — 11a: target / location / group state ───────────────
    /// Current target is actively casting the given spell id
    /// (`BotUnitSnapshot::is_casting && casting_spell_id == spell`). Use
    /// as an interrupt gate when a specific breakable cast is required
    /// (PB2's `TargetCastingSpellTrigger`).
    TargetCastingSpell(SpellId),
    /// Succeeds with `pct` chance (0..=100). Deterministic per `(tick,
    /// bot)` — rerolled each tick from `(server_time_ms, bot_handle)`.
    /// Good enough for jitter / random-emote / jump gating; strategies
    /// that need cryptographic entropy should draw from a real RNG on
    /// `BotState`. `pct == 0` always fails, `pct >= 100` always succeeds.
    RandomChance(u8),
    /// Bot's current world zone matches (`BotWorldSnapshot::zone_id`).
    InZone(u32),
    /// Bot's current world map matches
    /// (`BotWorldSnapshot::self_.pos.map_id`).
    InMap(u32),
    /// Group exists and has a marked unit for the bot's preferred assist
    /// raid target icon (`BotSettings::preferred_rti_icon`, default
    /// skull=7). Looks up `BotWorldSnapshot::group_raid_target_icons`.
    RtiAssistTargetValid,
    /// Group exists and has a marked unit for the bot's preferred CC
    /// raid target icon (`BotSettings::preferred_cc_rti_icon`, default
    /// square=5).
    RtiCcTargetValid,
    /// Bot is in a group with no tank assigned.
    PartyNoTank,
    /// Bot is in a group with no healer assigned.
    PartyNoHealer,
    /// Bot's master is within line of sight (`has_los`). Fails when no
    /// master is set.
    InLosOfMaster,
    /// Bot's master is within `DEFAULT_REACT_DISTANCE` yards
    /// (PB2 `aiReactDistance`, default 150y). Fails when no master is
    /// set. The distance is currently hardcoded — once `BotAiConfig` is
    /// plumbed through `TickContext`, this should read
    /// `ctx.ai_config.react_distance`.
    InReactRangeOfMaster,
    /// Bot's mode is `Follow` and a master guid is set. Use as a cheap
    /// precondition to formation/positioning leaves that only make
    /// sense while actively following a player.
    IsFollowingMaster,
    /// Bot's mode is `Follow` and the group has a tank that isn't this
    /// bot. Used by DPS/healer-specific follow-the-tank subtrees.
    IsFollowingTank,
    /// Bot has a configured protect target (`BotSettings::protect_target`)
    /// that is alive and below full HP. Gate for the protect-target
    /// reactive path.
    HasProtectTargetDamaged,

    // ── Conditions — 11b: target type discrimination ─────────────────────
    /// Current target is a player character. Decided from the target
    /// guid's high bits via `BotInterface::unit_kind` — no live Unit
    /// lookup, so this is cheap enough to gate PvP-only subtrees on
    /// every tick. Fails when no target is set.
    TargetIsPlayer,
    /// Current target is a pet (hunter/warlock pet, etc.). Same cheap
    /// guid-bit check as `TargetIsPlayer`. Used by pet-cleanup subtrees
    /// so CC/kill priority lists can deprioritize or skip pets.
    TargetIsPet,
    /// Current target is a critter (`Creature::IsCritter`). Unlike the
    /// player/pet variants, this requires resolving the live Creature
    /// on the bot's map, so prefer gating expensive subtrees with a
    /// cheaper check (target exists, is attackable) first. Used to
    /// suppress bot aggression on ambient wildlife.
    TargetIsCritter,

    // ── Conditions — 11c: party / dispel / res / consumables ────────────
    /// Some nearby party member carries a debuff the bot can dispel from
    /// the requested `DispelSchool`. Wraps
    /// `BotInterface::find_dispellable_target` with the school filter —
    /// pass `DispelSchool::Any` to match any school. Succeeds whenever a
    /// candidate exists (the actual spell pick lives in the dispel
    /// action leaf).
    PartyMemberNeedsDispel(DispelSchool),
    /// Some nearby party member is dead and can be resurrected. Wraps
    /// `BotInterface::find_dead_party_member`. Does not distinguish
    /// combat vs out-of-combat — class-specific res leaves (Rebirth
    /// vs Resurrection) handle that gating themselves.
    PartyMemberNeedsRes,
    /// Some party member (including self) has HP strictly below
    /// `threshold` (0.0..=1.0). Delegates to
    /// `crate::combat::targeting::find_heal_target`, so the scan order
    /// matches what the heal action leaves will actually cast on.
    /// `threshold = 1.0` succeeds as soon as anyone is off full HP.
    PartyMemberNeedsHeal(f32),
    /// Bot has at least one usable buff potion in bags (stat/damage
    /// elixir, e.g. Mongoose, Greater Arcane Elixir). Used as a gate
    /// before `UseBuffPotion` / `UseUtilityPotion` leaves. Calls
    /// `BotInterface::find_potion_in_bags(0)` which returns the item
    /// id of the first match (0 if none).
    HasBuffPotionAvailable,
    /// Bot's shared potion item cooldown (category 4) is ready. Pair
    /// with `HasBuffPotionAvailable` in a `Seq` to avoid wasting cast
    /// cycles on a potion that is still on cooldown.
    PotionCooldownReady,

    // ── Conditions — 11d: PvP / duel / reputation ───────────────────────
    /// Bot is currently PvP-flagged (`Player::IsPvP`). Gate for
    /// PvP-only strategies (battleground stay-on-target, world-PvP
    /// opportunistic aggression).
    PvpFlagged,
    /// Bot is in an active duel (duel_state == 2 — countdown has
    /// finished and the fight has started). Used to switch into
    /// dueling combat mode and suppress party-support reactions.
    InDuel,
    /// Bot has an open duel request / countdown (duel_state == 1 —
    /// `Player::duel` is set but `startTime == 0`). Gate for the
    /// auto-accept / decline decision layered on top of the `accept
    /// duel` setting.
    DuelRequested,
    /// Bot's reputation rank with `faction_id` is strictly below
    /// `rank`. Matches PB2's `RepWithFactionBelowTrigger` — used to
    /// pick faction-appropriate quests / vendors only while standing
    /// has room to grow. `ReputationRank::Neutral` is the default
    /// return when the bot has no record for the faction, so a
    /// `RepWithFactionBelow(f, Neutral)` always fails on an unknown
    /// faction (which is the correct PB2 behaviour — neutral isn't
    /// "below" neutral).
    RepWithFactionBelow(u32, ReputationRank),

    // ── Conditions — 11e: quest / recipe / item ─────────────────────────
    /// Bot's bags hold at least `n` copies of `item_id`. Wraps
    /// `BotInterface::bot_item_count` (backpack + equipped bags, not
    /// bank). Used by the reagent-availability gate in buff / enchant /
    /// consumable strategies that need to skip themselves when the bot
    /// is out of stock (e.g. priest Inner Fire without Holy Candles,
    /// mage Arcane Intellect without Arcane Powder). `n == 0` always
    /// succeeds — the count is compared with `>=`.
    ItemInBagsCount(ItemId, u32),
    /// Bot has learned `spell_id` as a recipe / ability. PB2 treats
    /// recipes and spells identically at the trigger layer (both live
    /// in the player's spell book), so this is just a `knows_spell`
    /// forwarder. Used by professions strategies that switch between
    /// "learn recipe" and "craft recipe" branches. Note that the
    /// `BotInterface::knows_spell` trait default returns `true` — the
    /// `Mock11a` test stub overrides it to check an explicit
    /// `known_spells` set so the negative branch is testable; custom
    /// mocks that exercise this variant should do the same.
    HasRecipe(SpellId),
    /// Quest with `quest_id` is currently accepted (entry present in
    /// the bot's quest log, regardless of completion state). PB2's
    /// `QuestInLogTrigger` — used by grind / travel strategies that
    /// only enable themselves while the corresponding quest is active,
    /// so the bot doesn't wander out to a quest zone for a quest it
    /// never picked up.
    QuestInLogActive(u32),
    /// Quest with `quest_id` is in the log AND is marked complete
    /// (objectives done, ready to turn in). PB2's
    /// `QuestCompletedTrigger` — gates the "head back to the quest
    /// giver" branch of travel strategies. A quest that isn't in the
    /// log at all returns Failure (use `QuestInLogActive` to
    /// distinguish "not started" from "in progress").
    QuestInLogComplete(u32),

    // ── Movement — encounter ─────────────────────────────────────────────
    /// Dodge an area effect — move to nearest safe position.
    FleeToSafe(f32),
    /// Move away from allies (exploding debuff).
    MoveAwayFromRaid(f32),
    /// Keep at least this distance from current target.
    MaintainRange(f32),
    /// Move behind current target (avoid cleave/tail).
    MoveBehind(f32),
    /// Stop all movement.
    HoldPosition,
    /// Move to encounter safe zone (Heigan dance).
    MoveToSafeZone,

    // ── Movement — world ─────────────────────────────────────────────────
    /// Stop bot movement.
    StopMoving,
    /// Follow group leader/tank at follow distance.
    Follow,
    /// Return to guard position.
    GuardReturn,

    // ── Combat — encounter ───────────────────────────────────────────────
    /// Cast a spell on self (GCD + cooldown gated).
    CastOnSelf(SpellId),
    /// Cast a spell on current target (GCD + cooldown gated).
    CastOnTarget(SpellId),
    /// Cast a heal on the lowest-HP group member (including self) below threshold.
    HealLowest(SpellId, f32),
    /// Cast a heal on the lowest-HP party member (not self) below threshold.
    HealInjuredParty(SpellId, f32),
    /// Move toward current target if farther than the specified distance.
    StickToTarget(f32),
    /// Cast a crowd-control spell on a nearby hostile that isn't the current target.
    CastCrowdControl(SpellId),
    /// Attack nearest hostile unit (add phases).
    AttackNearest,
    /// Taunt current target.
    Taunt,

    // ── Combat — reactive ────────────────────────────────────────────────
    /// Class-appropriate interrupt on current target.
    Interrupt,
    /// Find and dispel a debuff on a party member.
    DispelParty,
    /// Resurrect a dead party member (class-appropriate).
    ResurrectParty,
    /// Dump threat with class-appropriate ability.
    ThreatDump,
    /// Attack the focus target.
    FocusAttack,
    /// Tank: taunt loose adds not targeting us.
    TankPickupAdds,
    /// Assist: attack leader/tank's target.
    AssistLeader,
    /// Protect: attack mobs hitting the protect target.
    ProtectAttacker,

    // ── World actions ────────────────────────────────────────────────────
    /// Accept a pending resurrect from another player.
    AcceptResurrect,
    /// Move toward corpse position (ghost run).
    CorpseRun,
    /// Use spirit healer as last resort.
    UseSpiritHealer,
    /// Check if any group member is alive (for waiting for rez).
    HasAliveGroupMember,
    /// Record death timestamp if not already set (use in Seq with IsAlive.not()).
    RecordDeathTime,
    /// Check if the bot has been dead for less than N milliseconds.
    DeadForLessThan(u64),
    /// Mount up for travel.
    MountUp,
    /// Dismount.
    Dismount,
    /// Find and loot nearby corpses.
    LootNearest,
    /// Find vendor NPC, approach, sell grey items.
    VendorSellGrey,
    /// Find repair NPC, approach, repair equipment.
    RepairEquipment,
    /// Turn in a completed quest to nearby NPC.
    TurnInQuest,
    /// Accept available quests from nearby NPC.
    AcceptQuests,
    /// Attack a quest-relevant mob.
    AttackQuestMob,
    /// Move toward blackboard travel destination, clear on arrival.
    TravelToBlackboard,
    /// Select a travel destination based on the bot's current goals and
    /// write it to the blackboard. Used by quest/grind/wbuff modes to
    /// pick a destination before `TravelToBlackboard` navigates there.
    ChooseTravelTarget,
    /// Revive a dead pet.
    RevivePet,
    /// Summon pet if none exists.
    SummonPet,
    /// Feed unhappy pet (Hunter).
    FeedPet,
    /// Find and attack a level-appropriate mob for grinding.
    GrindTarget,

    // ── Actions — 11f: generic movement / positioning ───────────────────
    /// Kite away from the current target: if the bot is within `dist`
    /// yards of its target, move to a point on the line *target→bot*
    /// extended out to `dist * 2` yards. Mirrors PB2's kite behaviour
    /// for ranged classes (mage frost kite, warlock drain kite,
    /// hunter concussive kite). Returns `Failure` when there is no
    /// target, the bot is already outside `dist`, or pathing fails;
    /// returns `Running` while walking out. Strategies usually gate
    /// this on an attacker-in-melee condition so it doesn't fire
    /// while the bot is at a safe range already.
    KiteFromTarget(f32),
    /// Close to melee range on the current target: if the bot is
    /// farther than `dist` yards, move to the target's position.
    /// Counterpart to `KiteFromTarget` for melee engagers — mirrors
    /// the PB2 "close to target" reach leaves woven through the
    /// warrior / rogue / feral / DK rotations. Semantically
    /// equivalent to `StickToTarget(dist)` but kept as its own
    /// variant for strategy readability. Returns `Running` while
    /// moving, `Failure` when there is no target or the bot is
    /// already inside `dist`.
    CloseToTarget(f32),
    /// Ensure auto-attack / auto-shoot is engaged on the current target.
    /// Called each tick in the combat wrapper after targeting resolves.
    /// For ranged-equipped bots, calls `auto_shoot` to start/maintain
    /// Auto Shot (bow/gun/crossbow) or Shoot (wand). For melee bots
    /// (or when auto_shoot fails), calls `auto_attack(true)` to engage
    /// melee swings. Always returns Success so it doesn't block the
    /// combat pipeline Seq.
    EngageTarget,
    /// Initiate combat with the current target via the best available
    /// pull path. Dispatches in this order:
    ///   1. `auto_shoot` — wand/bow/gun/crossbow ranged auto-attack.
    ///      Cheapest path and the one PB2 prefers for mixed-class
    ///      groups because it doesn't burn a cooldown.
    ///   2. `taunt` — tank pull fallback for melee classes with a
    ///      taunt in their kit (warrior, druid bear, paladin).
    ///   3. `attack` — final fallback, engages melee auto-attack.
    /// Returns `Success` on the first path that reports success,
    /// `Failure` when every path bails. Class-specific "spell pulls"
    /// (e.g. hunter Hunter's Mark, warrior Charge, shaman Earthshock)
    /// are layered on top in the class file via a priority wrapper
    /// around `PullTarget`, not inside this leaf.
    PullTarget,

    // ── Actions — 11g: RTI / CC targeting ───────────────────────────────
    /// Mark the bot's current target with raid target icon `icon`
    /// (0..=7, star..skull). Wraps `group_set_target_icon` —
    /// broadcasts the icon to the whole group so Mangosbot's UI
    /// redraws. Used by tank / leader strategies to publish
    /// "kill target" marks. No-ops (Failure) when the bot is
    /// ungrouped, has no current target, or the icon is out of
    /// range. Callers that want "mark once and throttle" should
    /// wrap with `Throttle`.
    MarkRti(u8),
    /// Mark the current target with the bot's `preferred_rti_icon` setting.
    /// Returns Failure when the setting is `None` (user ran `rti clear`),
    /// so the throttle parent doesn't block subsequent ticks. This allows
    /// users to disable RTI marking at runtime without rebuilding the tree.
    MarkRtiPreferred,
    /// Mark the bot's current target with CC icon `icon`. Identical
    /// wire behaviour to `MarkRti` — kept as its own variant so CC
    /// strategies (mage sheep, priest shackle, warlock banish) can
    /// be picked out from assist marks in the tree and so throttle
    /// state is independent from the kill-mark throttle.
    MarkRtiCc(u8),
    /// Cast `spell` on the unit currently wearing the bot's
    /// `BotSettings::preferred_cc_rti_icon` (default square=5).
    /// Falls through to Failure when no mob wears the icon, the
    /// spell is on cooldown, or `can_cast` rejects the pairing.
    /// Used by CC-assignment strategies where the lead calls a
    /// target and each CC'er's tree picks its own icon.
    CcCastOnRti(SpellId),
    /// Cast `spell` on the nearest attackable unit that is NOT the
    /// bot's current target and does NOT already carry `spell`'s
    /// aura. Functionally equivalent to the existing
    /// `CastCrowdControl` variant — kept under its own name for
    /// strategy readability and to mirror the PB2 trigger
    /// vocabulary ("cc near" vs "cc rti").
    CcCastOnNearest(SpellId),
    /// Switch the bot's target to the unit wearing
    /// `BotSettings::preferred_rti_icon` (default skull=7) and
    /// start attacking it. PB2's "assist main tank" behaviour — the
    /// leader paints skull, every dps tree layers this leaf on top
    /// of their own target-pick so they converge. Failure when no
    /// mob wears the icon or the attack call is refused.
    RtiAssist,
    /// Switch the bot's target to the unit wearing
    /// `BotSettings::preferred_cc_rti_icon` and start attacking it.
    /// Used by the CC side of the same assist chain, so a CC'er
    /// can break early and re-engage the shackled / sheeped mob
    /// after the kill target drops.
    RtiCcTargetSelect,

    // ── Actions — Step 13: cross-class reactive combat ─────────────────
    /// Return to the pull-back position after pulling a mob.
    /// The pull-back point is typically the group's position or the
    /// tank's pre-pull location. Used by tank specs that have the
    /// `PULL_BACK` strategy flag to drag mobs back to the group
    /// instead of fighting at the pull point. Failure when no
    /// pull-back position is recorded or the bot is already there.
    PullBack,
    /// Wait for a pulled mob to reach melee range before engaging.
    /// Prevents the bot from charging out to meet the mob mid-pull,
    /// which would defeat the purpose of pulling back. Returns
    /// Running while waiting, Success once an attacker is in melee
    /// range (≤ 8 yards), Failure if no target exists.
    WaitForAttack,
    /// Pre-heal: cast a heal on the pull target / tank just before
    /// or as combat starts. Used by healer specs to front-load
    /// healing. Failure for non-healer roles or when no injured
    /// party member exists.
    PreHeal,
    /// Interrupt the bot's own cast-in-progress to react to an
    /// emergency (e.g. cancel a long heal to counterspell an enemy).
    /// Returns Success if a cast was interrupted, Failure if the bot
    /// was not casting.
    HealInterrupt,

    // ── Actions — 11h: consumables / racials / trinkets ────────────────
    /// Use a stat/damage buff elixir from the bot's bags. Looks up the
    /// first buff potion via `find_potion_in_bags(0)`, gates on
    /// `potion_cooldown_ready`, and consumes it via `use_item` on self.
    /// Strategies pair this with `HasBuffPotionAvailable` and
    /// `PotionCooldownReady` conditions so the BT short-circuits cheaply
    /// before hitting the use path.
    UseBuffPotion,
    /// Use a utility potion (Free Action, Swiftness, Invulnerability)
    /// from the bot's bags. Same path as `UseBuffPotion` but with
    /// `find_potion_in_bags(1)`. Utility potions share the potion
    /// category-4 cooldown.
    UseUtilityPotion,
    /// Cast a racial ability on self. Wraps `CastOnSelf` with an
    /// additional `knows_spell` gate so strategy files can
    /// unconditionally list racials without worrying about the bot's
    /// actual race. Example: Stoneform (20594), Berserking (20554),
    /// Arcane Torrent (28730), War Stomp (20549). Failure when the
    /// spell is unknown, on cooldown, or the GCD is active.
    UseRacial(SpellId),
    /// Activate the trinket in equipment slot `slot` (0 = top trinket,
    /// 1 = bottom trinket). Delegates to `BotInterface::use_trinket`
    /// which reads the equipped item, walks its OnUse spells, and
    /// fires the first ready one. Failure when the slot is empty, the
    /// trinket has no on-use effect, or everything is on cooldown.
    /// Strategies usually wrap this in `Throttle` gated on combat
    /// state.
    UseTrinket(u8),

    // ── Actions — 11i: social / group ──────────────────────────────────
    /// Accept a pending group/raid invitation. Failure when no invite
    /// is pending.
    AcceptGroupInvite,
    /// Leave the bot's current group/raid. Failure when not grouped.
    LeaveGroup,
    /// Accept a pending ready check. Failure when no check is active.
    AcceptReadyCheck,
    /// Accept a pending trade window. Failure when no trade is pending.
    AcceptTradeRequest,
    /// Accept an incoming duel request. Failure when duel_state != 1.
    AcceptDuelRequest,
    /// Decline an incoming duel request. Failure when duel_state != 1.
    DeclineDuelRequest,
    /// Accept a pending warlock/meeting-stone summon.
    AcceptSummon,
    /// Interact with a nearby meeting stone to queue for summoning.
    UseMeetingStone,

    // ── Actions — 11j: world interaction / economy (stubs) ────────────
    // These variants are placeholders. Each returns Failure until the
    /// Gossip with a specific NPC `entry`.
    Gossip(u32),
    /// Buy `qty` of `item_id` from a nearby vendor.
    BuyFromVendor(ItemId, u32),
    /// Mail an item to the master.
    MailItem,
    /// Check the bot's mailbox.
    CheckMail,
    /// Deposit items into the bank.
    BankDeposit,
    /// Withdraw items from the bank.
    BankWithdraw,
    /// Post an item on the auction house.
    AhPost,
    /// Bid on an auction house listing.
    AhBid,
    /// Roll on a loot item (need/greed/pass).
    LootRoll,
    /// Automatically roll on loot based on settings.
    AutoLootRoll,
    /// Share a quest with party members.
    ShareQuest,
    /// Learn all available spells from a nearby trainer.
    LearnTrainerSpells,
    /// Apply a saved talent build.
    ApplyTalentBuild,
    /// Equip an item by id.
    EquipItem(ItemId),
    /// Unequip an item slot.
    UnequipSlot(u8),
    /// Apply a saved outfit preset.
    ApplyOutfit,
    /// Cast fishing and wait for catch.
    Fish,
    /// Play a random emote (non-RPG context — e.g. idle chatter).
    RandomEmote,
    /// Say a random message (RP phrases, idle chatter).
    RandomSay,
    /// Apply all missing world buffs from config (AddAura for each).
    ApplyWorldBuffs,
    /// Travel to a world buff location for `buff_id`.
    WorldBuffTravel(SpellId),
    /// Consume the next entry in the RTSC move queue.
    RtscConsumeMoveQueue,
    /// Join LFG queue (WotLK only).
    LfgJoin,
    /// Accept LFG proposal (WotLK only).
    LfgAccept,
    /// Accept a pending battleground invite.
    AcceptBgInvite,
    /// Queue for a battleground.
    QueueBg,
    /// Defend a BG base/node.
    DefendBase,
    /// Capture a BG flag.
    CaptureFlag,
    /// Return a dropped friendly flag.
    ReturnFlag,
    /// Assault a BG base/node.
    AssaultBase,
    /// Arena: initial engage positioning setup.
    ArenaEngageSetup,
    /// Arena: peel for a teammate under pressure.
    ArenaPeel,
    /// Dungeon: stay within range of the tank.
    DungeonStayNearTank,
    /// Dungeon: avoid breaking CC'd mobs.
    DungeonAvoidBreakingCc,
    /// Dump debug state (kind 0=full, 1=strategies, 2=blackboard).
    DebugDumpState(u8),

    // ── Battleground ─────────────────────────────────────────────────────
    /// True if the bot is in a battleground.
    InBattleground,
    /// Capture a nearby BG objective (flag, base).
    BgCaptureObjective,
    /// Attack a nearby enemy player in BG.
    BgAttackEnemy,

    // ── RPG ──────────────────────────────────────────────────────────────
    /// Wander to a random nearby point.
    RpgWander,
    /// Interact with a nearby gossip NPC.
    RpgInteractNpc,
    /// Play a random emote.
    RpgEmote,

    // ── Gathering ────────────────────────────────────────────────────────
    /// True if the bot has a gathering profession.
    HasGatheringSkill,
    /// Find and gather a nearby node (herb, ore, skin).
    GatherNode,

    // ── Noncombat ────────────────────────────────────────────────────────
    /// Eat/drink to recover HP/mana (returns Running while recovering).
    Consumables,
    /// Apply missing buffs to self/party from the given buff list.
    Buff(&'static [GroupBuff]),
    /// Bridge to encounter FSM — delegates to the active encounter's phase BT.
    EncounterOverride,

    // ── Class preferences (data lives on BotSettings::class_prefs) ───────
    /// Rogue: apply the configured weapon poisons (main and off hand) if
    /// missing. Reads `ClassPrefs::Rogue`. No-op on non-rogues or when no
    /// rank of the configured poison is known.
    ApplyPoisons,
    /// Shaman: drop the configured totem set — one per school, only for
    /// slots the player has filled. Reads `ClassPrefs::Shaman`. No-op on
    /// non-shamans or when no rank of the configured totem is known.
    DropConfiguredTotems,
    /// Shaman: apply the configured weapon imbues (Rockbiter / Flametongue
    /// / Frostbrand / Windfury / Earthliving) to main- and off-hand.
    ApplyShamanImbues,
    /// Paladin: maintain the configured self-aura (Devotion / Retribution
    /// / Concentration / …). Re-cast whenever the current aura differs.
    MaintainPaladinAura,
    /// Paladin: cast the configured Blessing on any party member missing
    /// it. Prefers the Greater variant when the player opted in and the
    /// bot knows it.
    ApplyConfiguredBlessings,
    /// Hunter: maintain the configured Aspect (Hawk / Viper / …). Re-cast
    /// whenever the current Aspect differs from the configured one.
    MaintainHunterAspect,
    /// Warlock: keep the configured Curse on the current target.
    MaintainConfiguredCurse,
    /// Warrior: if `forced_stance` is set and not already active, swap
    /// into it. No-op when no stance is forced.
    EnforceWarriorStance,

    // ── Instance / sub-area gating ───────────────────────────────────────
    /// Succeeds iff the bot's current `area_id` (from `BotWorldSnapshot`)
    /// matches. Used by instance FSMs to scope zone-wide behaviors to a
    /// specific sub-area (e.g. BWL suppression corridor).
    InArea(u32),
    /// Succeeds iff the bot's current `area_id` is contained in the slice.
    /// Use for behaviors that span several adjacent sub-areas (e.g. the
    /// MC rune rooms before Majordomo).
    InAnyArea(&'static [u32]),

    // ── Generic escape hatch ─────────────────────────────────────────────
    /// Invokes a [`BehaviorLeaf`] as a BT leaf. This is the plugin point
    /// that lets instance-specific gameplay logic (BWL suppression
    /// disarm, MC rune dousing, Karazhan Chess moves, …) live inside
    /// its own instance module without adding a new `Bt` variant for
    /// every case. `BehaviorLeaf` is `Copy`, so this variant preserves
    /// the enum's derives.
    Custom(BehaviorLeaf),
}

/// Plugin leaf for instance-specific behavior.
///
/// Holds a handler fn plus lightweight metadata. Declared as a
/// top-level `const` in the instance module that owns the behavior,
/// then referenced from that module's `zone_wide_bt()` or per-phase
/// tree via [`Bt::Custom`]. Keeping this as a struct (rather than a
/// bare fn pointer tuple) lets new per-leaf metadata — display text,
/// priority hints, cooldown tags — be added without touching every
/// call site.
///
/// # Fields
/// - `label`: internal debug / trace identifier (e.g.
///   `"bwl_disarm_device"`). Never shown to players.
/// - `handler`: the tick function. Receives the current
///   [`TickContext`] and returns a [`BtResult`].
/// - `display_text`: optional user-facing status string (e.g.
///   `"Disarming Suppression Device"`). Currently stored only;
///   consumers (chat status, debug overlay) can read it via the
///   public field.
#[derive(Debug, Clone, Copy)]
pub struct BehaviorLeaf {
    pub label: &'static str,
    pub handler: fn(&mut TickContext<'_>) -> BtResult,
    pub display_text: Option<&'static str>,
}

impl Bt {
    pub fn not(self) -> Self {
        Bt::Not(Box::new(self))
    }

    /// Build a throttle node. The caller's source location is captured via
    /// `#[track_caller]` and used as the state key on the bot — each unique
    /// `throttle()` call site becomes an independent throttle slot.
    #[track_caller]
    pub fn throttle(interval_ms: u64, child: Self) -> Self {
        let loc = std::panic::Location::caller();
        Bt::Throttle {
            key: ThrottleKey {
                file: loc.file(),
                line: loc.line(),
            },
            interval_ms,
            child: Box::new(child),
        }
    }

    /// Run `self` only when `guard` succeeds. Equivalent to `Seq(vec![guard, self])`.
    ///
    /// Reads top-down: `CastOnSelf(ICE_BLOCK).when(Cmp(SelfHealthPct, Below(20)))`.
    pub fn when(self, guard: Bt) -> Bt {
        Bt::Seq(vec![guard, self])
    }

    /// Try `self`; if it fails, fall back to `fallback`. Equivalent to `Sel(vec![self, fallback])`.
    pub fn or_else(self, fallback: Bt) -> Bt {
        Bt::Sel(vec![self, fallback])
    }

    // ── Aura helper constructors ────────────────────────────────────────
    //
    // The underlying data model is `Bt::Aura { unit, key, pred }`; these
    // free functions keep call sites terse and readable. See the design
    // note at the `Bt::Aura` variant definition.

    /// Bot is missing `spell` on itself.
    pub fn self_missing(spell: SpellId) -> Bt {
        Bt::Aura {
            unit: AuraUnit::Self_,
            key: AuraKey::Spell(spell),
            pred: AuraPred::Missing,
        }
    }

    /// Bot has `spell` on itself.
    pub fn self_has(spell: SpellId) -> Bt {
        Bt::Aura {
            unit: AuraUnit::Self_,
            key: AuraKey::Spell(spell),
            pred: AuraPred::Present,
        }
    }

    /// Current target has `spell`.
    pub fn target_has(spell: SpellId) -> Bt {
        Bt::Aura {
            unit: AuraUnit::Target,
            key: AuraKey::Spell(spell),
            pred: AuraPred::Present,
        }
    }

    /// Current target is missing `spell`.
    pub fn target_missing(spell: SpellId) -> Bt {
        Bt::Aura {
            unit: AuraUnit::Target,
            key: AuraKey::Spell(spell),
            pred: AuraPred::Missing,
        }
    }

    /// Bot is missing every rank in `ranks` on itself.
    pub fn self_missing_any_rank(ranks: &'static [SpellId]) -> Bt {
        Bt::Aura {
            unit: AuraUnit::Self_,
            key: AuraKey::AnyRank(ranks),
            pred: AuraPred::Missing,
        }
    }

    /// Current target is missing every rank in `ranks`.
    pub fn target_missing_any_rank(ranks: &'static [SpellId]) -> Bt {
        Bt::Aura {
            unit: AuraUnit::Target,
            key: AuraKey::AnyRank(ranks),
            pred: AuraPred::Missing,
        }
    }

    /// Current target has fewer than `max` stacks of `spell`
    /// (missing counts as 0).
    pub fn target_aura_stacks_below(spell: SpellId, max: u8) -> Bt {
        Bt::Aura {
            unit: AuraUnit::Target,
            key: AuraKey::Spell(spell),
            pred: AuraPred::StacksBelow(max),
        }
    }
}

// ── BtNode implementation ───────────────────────────────────────────────────

impl BtNode for Bt {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        let result = match self {
            // ── Compositors ──────────────────────────────────────────────
            Bt::Seq(children) => {
                let mut last_i = 0;
                let mut res = BtResult::Success;
                // Snapshot each child's trace so earlier children's paths
                // aren't overwritten when a later child's Sel clears the
                // trace.  Only collected when monitor tracing is active.
                let mut child_traces: Vec<Vec<String>> = Vec::new();
                for (i, child) in children.iter().enumerate() {
                    last_i = i;
                    match child.tick(ctx) {
                        BtResult::Success => {
                            if let Some(ref trace) = ctx.monitor_trace {
                                let snapshot = trace.borrow().clone();
                                child_traces.push(snapshot);
                            }
                        }
                        other => {
                            if let Some(ref trace) = ctx.monitor_trace {
                                let snapshot = trace.borrow().clone();
                                child_traces.push(snapshot);
                            }
                            res = other;
                            break;
                        }
                    }
                }
                if let Some(ref trace) = ctx.monitor_trace {
                    if res != BtResult::Failure {
                        // Reconstruct a combined trace showing all children.
                        // Format: "Seq[last] > child_trace" for the last
                        // child, with earlier children appended as
                        // " || Seq[i] > ..." segments.
                        let mut combined = Vec::new();
                        combined.push(format!("Seq[{last_i}]"));
                        if let Some(last) = child_traces.last() {
                            combined.extend(last.iter().cloned());
                        }
                        // Prepend earlier children's traces so the primary
                        // action (Seq[0]) is visible.
                        if child_traces.len() > 1 {
                            for (ci, ct) in child_traces.iter().enumerate() {
                                if ci == child_traces.len() - 1 {
                                    break;
                                }
                                let child_path = if ct.is_empty() {
                                    String::from("(empty)")
                                } else {
                                    ct.join(" > ")
                                };
                                combined.push(format!("|| Seq[{ci}]={child_path}"));
                            }
                        }
                        *trace.borrow_mut() = combined;
                    }
                }
                return res;
            }
            Bt::Sel(children) => {
                for (i, child) in children.iter().enumerate() {
                    // Clear trace before each child so failed branches
                    // don't leave stale entries.
                    if let Some(ref trace) = ctx.monitor_trace {
                        trace.borrow_mut().clear();
                    }
                    match child.tick(ctx) {
                        BtResult::Failure => {}
                        other => {
                            if let Some(ref trace) = ctx.monitor_trace {
                                trace.borrow_mut().insert(0, format!("Sel[{i}]"));
                            }
                            return other;
                        }
                    }
                }
                return BtResult::Failure;
            }
            Bt::Not(child) => {
                let r = match child.tick(ctx) {
                    BtResult::Success => BtResult::Failure,
                    BtResult::Failure => BtResult::Success,
                    other @ BtResult::Running => other,
                };
                if let Some(ref trace) = ctx.monitor_trace {
                    if r != BtResult::Failure {
                        trace.borrow_mut().insert(0, "Not".to_string());
                    }
                }
                return r;
            }
            Bt::Optional(child) => {
                let _ = child.tick(ctx);
                // Always succeed so parent Seq continues.
                if let Some(ref trace) = ctx.monitor_trace {
                    trace.borrow_mut().insert(0, "Optional".to_string());
                }
                return BtResult::Success;
            }
            Bt::Throttle {
                key,
                interval_ms,
                child,
            } => {
                let now = ctx.server_time_ms;
                // Running-transparency: if the child is already in a Running
                // phase (previous tick returned Running), bypass the cooldown
                // check and tick it again so the in-progress action can
                // continue. Otherwise a long-running move_to would be
                // interrupted by the throttle returning Failure, causing the
                // parent `Sel` to fall through to a sibling arm
                // (e.g. `StopMoving`) and halt the bot mid-movement.
                if !ctx.throttles.is_running(*key) {
                    let last = ctx.throttles.last_fire(*key);
                    if now.saturating_sub(last) < *interval_ms {
                        return BtResult::Failure;
                    }
                }
                let result = child.tick(ctx);
                match result {
                    BtResult::Running => {
                        ctx.throttles.mark_fired(*key, now);
                        ctx.throttles.set_running(*key, true);
                    }
                    BtResult::Success => {
                        ctx.throttles.mark_fired(*key, now);
                        ctx.throttles.set_running(*key, false);
                    }
                    BtResult::Failure => {
                        ctx.throttles.set_running(*key, false);
                    }
                }
                if let Some(ref trace) = ctx.monitor_trace {
                    if result != BtResult::Failure {
                        trace.borrow_mut().insert(0, "Throttle".to_string());
                    }
                }
                return result;
            }

            // ── Conditions — encounter ───────────────────────────────────
            Bt::Aura { unit, key, pred } => {
                let target = match unit {
                    AuraUnit::Self_ => Some(ctx.bot_handle),
                    AuraUnit::Target => ctx.current_target(),
                };
                let Some(t) = target else {
                    return BtResult::Failure;
                };
                let result = match (key, pred) {
                    (AuraKey::Spell(id), AuraPred::Present) => ctx.interface.has_aura(t, *id),
                    (AuraKey::Spell(id), AuraPred::Missing) => !ctx.interface.has_aura(t, *id),
                    (AuraKey::Spell(id), AuraPred::StacksBelow(max)) => ctx
                        .interface
                        .get_aura(t, *id)
                        .is_none_or(|a| a.stacks < *max),
                    (AuraKey::AnyRank(ranks), AuraPred::Present) => {
                        crate::engine::aura_helpers::has_any_rank(ctx.interface, t, ranks)
                    }
                    (AuraKey::AnyRank(ranks), AuraPred::Missing) => {
                        !crate::engine::aura_helpers::has_any_rank(ctx.interface, t, ranks)
                    }
                    (AuraKey::AnyRank(_), AuraPred::StacksBelow(_)) => false,
                };
                ok(result)
            }
            Bt::KnowsSpell(spell) => ok(ctx.interface.knows_spell(*spell)),
            Bt::TargetIsCasting => ok(ctx
                .current_target()
                .is_some_and(|t| ctx.interface.get_unit_snapshot(t).is_casting)),
            Bt::IsClass(class) => ok(ctx.class == *class),
            Bt::IsTank => ok(ctx.is_tank()),
            Bt::IsRanged => ok(ctx.is_ranged_or_healer()),
            Bt::IsMeleeDps => ok(!ctx.is_ranged_or_healer() && !ctx.is_tank()),
            Bt::GroupMembersBelow(count, threshold) => {
                ok(count_group_members_below(ctx, *threshold) >= *count)
            }

            // ── Conditions — general ─────────────────────────────────────
            Bt::Noop => BtResult::Success,
            Bt::InCombat => ok(ctx.in_combat()),
            Bt::IsAlive => ok(ctx.snap.self_.is_alive),
            Bt::IsGhost => ok(ctx.snap.self_.is_ghost),
            Bt::OnTaxi => ok(ctx.snap.self_.on_taxi),
            Bt::IsMounted => ok(ctx.interface.is_mounted()),
            Bt::IsIndoor => ok(ctx.interface.is_indoor()),
            Bt::IsMoving => ok(ctx.snap.self_.is_moving),
            Bt::ModeIs(mode) => ok(ctx.settings.mode == *mode),
            // True if *any* of the four per-state strategy engines has
            // this flag set. Typed filters (`@nc=`, `@co=`, `@react=`,
            // `@dead=`) key on a specific slot via the chat-filter layer;
            // `StrategyEnabled` is the cross-state runtime gate used by
            // mode dispatch and subtrees that do not care which engine
            // owns the flag.
            Bt::StrategyEnabled(flags) => ok(ctx.settings.strategies.has_any(*flags)),
            Bt::ReactivityIs(r) => ok(ctx.settings.reactivity == *r),
            Bt::UsesMana => ok(ctx.snap.self_.power_type == 0),
            Bt::Cmp(res, op) => ok(eval_cmp(ctx, *res, *op)),
            Bt::IsBehindTarget => ok(ctx
                .current_target()
                .is_some_and(|t| ctx.interface.bot_is_behind(t))),
            Bt::MainHandIs(wt) => {
                ok(ctx.interface.bot_equipped_weapon_subclass(0) == wt.subclass())
            }
            Bt::OffHandIs(wt) => {
                ok(ctx.interface.bot_equipped_weapon_subclass(1) == wt.subclass())
            }
            Bt::RangedIs(wt) => {
                ok(ctx.interface.bot_equipped_weapon_subclass(2) == wt.subclass())
            }
            Bt::InShapeshift(form) => ok(ctx.snap.self_.shapeshift_form == *form),
            // ITEM_ID_SOUL_SHARD = 6265 (classic through wotlk).
            Bt::SoulShardsAtLeast(n) => {
                ok(ctx.interface.bot_item_count(ItemId(6265)) >= u32::from(*n))
            }
            Bt::HasTotem(slot) => {
                ok((ctx.interface.bot_active_totem_mask() & (1u8 << (*slot & 3))) != 0)
            }
            Bt::MainHandEnchanted => ok(ctx.interface.bot_weapon_enchanted(0)),
            Bt::OffHandEnchanted => ok(ctx.interface.bot_weapon_enchanted(1)),
            Bt::RunesReady(n) => {
                ok(ctx.interface.bot_runes_ready_mask().count_ones() >= u32::from(*n))
            }
            Bt::SettingEnabled(s) => ok(check_setting(ctx, *s)),
            Bt::HasFocusTarget => ok(ctx.settings.focus_target.is_some()),
            Bt::HasProtectTarget => ok(ctx.settings.protect_target.is_some()),
            Bt::HasAttackers => ok(!ctx.attackers.is_empty()),
            Bt::HasNearby => ok(!ctx.nearby.is_empty()),
            Bt::HasTravelDest => {
                use crate::engine::blackboard::Key;
                ok(ctx.blackboard.get_f32(Key::TravelDestX).is_some())
            }
            Bt::HasSellableItems => ok(ctx.interface.has_sellable_items()),
            Bt::DurabilityBelow(pct) => ok(ctx.interface.get_durability_pct() < *pct),
            Bt::TargetCastingInterruptible => ok(ctx
                .current_target()
                .is_some_and(|t| ctx.interface.is_casting_interruptible(t))),
            Bt::PullingAggro => ok(tick_pulling_aggro(ctx)),
            Bt::IsPetClass => ok(matches!(
                ctx.class,
                PlayerClass::Hunter | PlayerClass::Warlock
            )),
            Bt::HasPet => ok(ctx.interface.has_pet()),
            Bt::PetAlive => ok(ctx.interface.pet_is_alive()),
            Bt::PetUnhappy => ok(ctx.interface.pet_happiness() < 2),
            Bt::ShouldEngage => {
                let result = match ctx.settings.reactivity {
                    Reactivity::Passive => false,
                    Reactivity::Defensive => !ctx.attackers.is_empty(),
                    Reactivity::Aggressive => !ctx.attackers.is_empty() || !ctx.nearby.is_empty(),
                };
                if !result {
                    ctx.monitor(format_args!(
                        "ENGAGE: ShouldEngage=false react={:?} attackers={} nearby={}",
                        ctx.settings.reactivity,
                        ctx.attackers.len(),
                        ctx.nearby.len(),
                    ));
                }
                ok(result)
            }
            Bt::TargetCastingSpell(spell) => ok(ctx.current_target().is_some_and(|t| {
                let s = ctx.interface.get_unit_snapshot(t);
                s.is_casting && s.casting_spell_id == spell.raw()
            })),
            Bt::RandomChance(pct) => ok(eval_random_chance(ctx, *pct)),
            Bt::InZone(zone) => ok(ctx.snap.zone_id == *zone),
            Bt::InMap(map) => ok(ctx.snap.self_.pos.map_id == *map),
            Bt::RtiAssistTargetValid => {
                let icon = ctx.settings.preferred_rti_icon.unwrap_or(7);
                ok((icon as usize) < 8 && ctx.snap.group_raid_target_icons[icon as usize] != 0)
            }
            Bt::RtiCcTargetValid => {
                let icon = ctx.settings.preferred_cc_rti_icon.unwrap_or(5);
                ok((icon as usize) < 8 && ctx.snap.group_raid_target_icons[icon as usize] != 0)
            }
            Bt::PartyNoTank => {
                ok(ctx.snap.group_size > 0 && ctx.interface.group_get_tank().is_none())
            }
            Bt::PartyNoHealer => {
                ok(ctx.snap.group_size > 0 && ctx.interface.group_get_healer().is_none())
            }
            Bt::InLosOfMaster => match ctx.master_guid {
                Some(m) if m != 0 => ok(ctx.interface.has_los(m)),
                _ => BtResult::Failure,
            },
            Bt::InReactRangeOfMaster => match ctx.master_guid {
                Some(m) if m != 0 => {
                    ok(ctx.interface.unit_distance(m) <= DEFAULT_REACT_DISTANCE)
                }
                _ => BtResult::Failure,
            },
            Bt::IsFollowingMaster => ok(ctx.settings.mode == BehaviorMode::Follow
                && ctx.master_guid.is_some_and(|m| m != 0)),
            Bt::IsFollowingTank => {
                if ctx.settings.mode != BehaviorMode::Follow {
                    return BtResult::Failure;
                }
                match ctx.interface.group_get_tank() {
                    Some(t) if t != ctx.bot_handle => BtResult::Success,
                    _ => BtResult::Failure,
                }
            }
            Bt::HasProtectTargetDamaged => {
                let Some(t) = ctx.settings.protect_target else {
                    return BtResult::Failure;
                };
                let s = ctx.interface.get_unit_snapshot(t);
                ok(s.is_alive && s.max_health > 0 && s.health < s.max_health)
            }

            Bt::TargetIsPlayer => match ctx.current_target() {
                Some(t) => ok(ctx.interface.unit_kind(t) == 1),
                None => BtResult::Failure,
            },
            Bt::TargetIsPet => match ctx.current_target() {
                Some(t) => ok(ctx.interface.unit_kind(t) == 2),
                None => BtResult::Failure,
            },
            Bt::TargetIsCritter => match ctx.current_target() {
                Some(t) => ok(ctx.interface.unit_kind(t) == 3),
                None => BtResult::Failure,
            },

            // ── Conditions — 11c ─────────────────────────────────────
            Bt::PartyMemberNeedsDispel(school) => {
                ok(ctx.interface.find_dispellable_target(school.mask()).is_some())
            }
            Bt::PartyMemberNeedsRes => {
                ok(ctx.interface.find_dead_party_member().is_some())
            }
            Bt::PartyMemberNeedsHeal(threshold) => {
                ok(crate::combat::targeting::find_heal_target(ctx, *threshold).is_some())
            }
            Bt::HasBuffPotionAvailable => {
                ok(ctx.interface.find_potion_in_bags(0).0 != 0)
            }
            Bt::PotionCooldownReady => ok(ctx.interface.potion_cooldown_ready()),

            // ── Conditions — 11d ─────────────────────────────────────
            Bt::PvpFlagged => ok(ctx.interface.is_pvp_flagged()),
            Bt::InDuel => ok(ctx.interface.duel_state() == 2),
            Bt::DuelRequested => ok(ctx.interface.duel_state() == 1),
            Bt::RepWithFactionBelow(faction, rank) => {
                let current = ctx.interface.reputation_rank(*faction);
                // 255 is the "faction id not in DBC" sentinel — treat
                // as "no data", which means the threshold can't be
                // evaluated and we report Failure (avoid triggering
                // a grind for a non-existent faction).
                ok(current != 255 && current < rank.raw())
            }

            // ── Conditions — 11e ─────────────────────────────────────
            Bt::ItemInBagsCount(item, n) => {
                ok(ctx.interface.bot_item_count(*item) >= *n)
            }
            Bt::HasRecipe(spell) => ok(ctx.interface.knows_spell(*spell)),
            Bt::QuestInLogActive(quest_id) => {
                ok(ctx
                    .interface
                    .get_quest_log()
                    .iter()
                    .any(|q| q.quest_id == *quest_id))
            }
            Bt::QuestInLogComplete(quest_id) => {
                ok(ctx
                    .interface
                    .get_quest_log()
                    .iter()
                    .any(|q| q.quest_id == *quest_id && q.complete))
            }

            Bt::ShouldFlee => {
                // Command-driven override takes precedence.
                if ctx.server_time_ms < ctx.settings.flee_override_until_ms {
                    return BtResult::Success;
                }
                // Strategy-gated HP-triggered flee.
                let threshold = ctx.settings.flee_hp_pct;
                if threshold > 0.0
                    && ctx
                        .settings
                        .strategies
                        .has_any(crate::bot::settings::StrategyFlags::FLEE)
                    && ctx.self_hp_pct() < threshold
                {
                    return BtResult::Success;
                }
                BtResult::Failure
            }

            // ── Movement — encounter ─────────────────────────────────────
            Bt::FleeToSafe(radius) => move_to_safe(ctx, *radius),
            Bt::MoveAwayFromRaid(dist) => move_to_safe(ctx, *dist),
            Bt::MaintainRange(min_range) => {
                let (too_close, dist) = ctx
                    .current_target()
                    .map(|t| {
                        let d = ctx.interface.unit_distance(t);
                        (d < *min_range, d)
                    })
                    .unwrap_or((false, 0.0));
                if !too_close {
                    return BtResult::Failure;
                }
                ctx.monitor(format_args!(
                    "MOVE: MaintainRange({min_range}) dist={dist:.1}y -> flee",
                ));
                move_to_safe(ctx, *min_range * 2.0)
            }
            Bt::MoveBehind(distance) => {
                let target = match ctx.current_target() {
                    Some(t) => t,
                    None => return BtResult::Failure,
                };
                // Already behind and within melee range — nothing to do.
                if ctx.interface.bot_is_behind(target)
                    && ctx.interface.unit_distance(target) <= 5.0
                {
                    return BtResult::Failure;
                }
                // Use chase with angle=PI to smoothly track behind the target
                // instead of move_to which restarts splines every tick.
                if ctx.interface.chase(target, *distance, std::f32::consts::PI) {
                    BtResult::Running
                } else {
                    BtResult::Failure
                }
            }
            Bt::HoldPosition | Bt::StopMoving => {
                ctx.interface.stop_moving();
                BtResult::Success
            }
            Bt::MoveToSafeZone => {
                use crate::engine::blackboard::Key;
                let _zone = ctx.blackboard.get_u32(Key::EncounterSafeZone).unwrap_or(1);
                move_to_safe(ctx, 10.0)
            }

            // ── Movement — world ─────────────────────────────────────────
            Bt::Follow => tick_follow(ctx),
            Bt::GuardReturn => tick_guard_return(ctx),

            // ── Combat — encounter ───────────────────────────────────────
            Bt::CastOnSelf(spell) => cast(ctx, *spell, ctx.bot_handle),
            Bt::CastOnTarget(spell) => match ctx.current_target() {
                Some(t) => cast(ctx, *spell, t),
                None => BtResult::Failure,
            },
            Bt::HealLowest(spell, threshold) => {
                match crate::combat::targeting::find_heal_target(ctx, *threshold) {
                    Some(t) => cast(ctx, *spell, t),
                    None => BtResult::Failure,
                }
            }
            Bt::HealInjuredParty(spell, threshold) => {
                match crate::combat::targeting::find_injured_party_member(ctx, *threshold) {
                    Some(t) => cast(ctx, *spell, t),
                    None => BtResult::Failure,
                }
            }
            Bt::StickToTarget(range) => {
                if ctx.snap.self_.is_casting {
                    ctx.monitor(format_args!("MOVE: StickToTarget({range}) SKIP (casting)"));
                    return BtResult::Failure;
                }
                if ctx.settings.mode == BehaviorMode::Stay {
                    return BtResult::Failure;
                }
                let target = match ctx.current_target() {
                    Some(t) => t,
                    None => {
                        ctx.monitor(format_args!("MOVE: StickToTarget({range}) FAIL (no target)"));
                        return BtResult::Failure;
                    }
                };
                let dist = ctx.interface.unit_distance(target);
                if dist <= *range {
                    return BtResult::Failure;
                }
                ctx.monitor(format_args!(
                    "MOVE: StickToTarget({range}) dist={dist:.1}y -> chase 0x{target:X}",
                ));
                if ctx.interface.chase(target, *range, 0.0) {
                    BtResult::Running
                } else {
                    BtResult::Failure
                }
            }
            Bt::CastCrowdControl(spell) => {
                let current = ctx.current_target().unwrap_or(0);
                let victim = ctx.nearby.iter().copied().find(|&u| {
                    u != current
                        && !ctx.interface.has_aura(u, *spell)
                        && ctx.interface.can_cast(*spell, u)
                });
                match victim {
                    Some(t) => cast(ctx, *spell, t),
                    None => BtResult::Failure,
                }
            }
            Bt::AttackNearest => {
                let target = ctx.attackers.first().or_else(|| ctx.nearby.first());
                match target {
                    Some(&unit) if ctx.current_target() == Some(unit) => {
                        ctx.pending_target.set(Some(unit));
                        BtResult::Success
                    }
                    Some(&unit) if ctx.attack(unit) => BtResult::Success,
                    _ => BtResult::Failure,
                }
            }
            Bt::Taunt => match ctx.current_target() {
                Some(t) if ctx.interface.taunt(t) => BtResult::Success,
                _ => BtResult::Failure,
            },

            // ── Combat — reactive ────────────────────────────────────────
            Bt::Interrupt => tick_interrupt(ctx),
            Bt::DispelParty => tick_dispel(ctx),
            Bt::ResurrectParty => tick_resurrect(ctx),
            Bt::ThreatDump => tick_threat_dump(ctx),
            Bt::FocusAttack => tick_focus_attack(ctx),
            Bt::TankPickupAdds => tick_tank_pickup(ctx),
            Bt::AssistLeader => tick_assist_leader(ctx),
            Bt::ProtectAttacker => tick_protect(ctx),

            // ── World actions ────────────────────────────────────────────
            Bt::AcceptResurrect => {
                if ctx.interface.accept_resurrect() {
                    use crate::engine::blackboard::Key;
                    ctx.blackboard.clear(Key::DeathTimestampMs);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::CorpseRun => {
                if let Some(pos) = ctx.interface.get_corpse_position()
                    && ctx.interface.move_to(pos.x, pos.y, pos.z)
                {
                    return BtResult::Running;
                }
                BtResult::Failure
            }
            Bt::UseSpiritHealer => {
                if ctx.interface.use_spirit_healer() {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::HasAliveGroupMember => {
                // If the bot is in a group, assume there may be alive members
                // who could resurrect. PB2 checks for alive party members;
                // here we use group_size as a proxy since the snapshot doesn't
                // expose individual member alive/dead states in the tick path.
                ok(ctx.snap.group_size > 0)
            }
            Bt::RecordDeathTime => {
                use crate::engine::blackboard::{Key, Value};
                // Only record once per death.
                if ctx.blackboard.get_u64(Key::DeathTimestampMs).is_none() {
                    ctx.blackboard.set(Key::DeathTimestampMs, Value::U64(ctx.server_time_ms));
                }
                BtResult::Success
            }
            Bt::DeadForLessThan(max_ms) => {
                use crate::engine::blackboard::Key;
                match ctx.blackboard.get_u64(Key::DeathTimestampMs) {
                    Some(death_ms) => {
                        let elapsed = ctx.server_time_ms.saturating_sub(death_ms);
                        ok(elapsed < *max_ms)
                    }
                    // No recorded death time — treat as "just died"
                    None => BtResult::Success,
                }
            }
            Bt::MountUp => {
                if ctx.interface.mount_up() {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::Dismount => {
                if ctx.interface.dismount() {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::LootNearest => tick_loot(ctx),
            Bt::LootRoll => tick_loot_roll(ctx),
            Bt::AutoLootRoll => tick_auto_loot_roll(ctx),
            Bt::CheckMail => tick_check_mail(ctx),
            Bt::VendorSellGrey => tick_vendor(ctx),
            Bt::RepairEquipment => tick_repair(ctx),
            Bt::TurnInQuest => tick_turn_in_quest(ctx),
            Bt::AcceptQuests => tick_accept_quests(ctx),
            Bt::AttackQuestMob => tick_attack_quest_mob(ctx),
            Bt::LearnTrainerSpells => tick_learn_trainer_spells(ctx),
            Bt::ApplyTalentBuild => tick_apply_talent_build(ctx),
            Bt::TravelToBlackboard => tick_travel(ctx),
            Bt::ChooseTravelTarget => tick_choose_travel_target(ctx),
            Bt::RevivePet => {
                // Don't restart if already casting (Revive Pet has a cast time).
                if ctx.snap.self_.is_casting {
                    return BtResult::Failure;
                }
                if ctx.interface.revive_pet() {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::SummonPet => {
                // Don't restart if already casting (Summon Demon is a 10s cast).
                if ctx.snap.self_.is_casting {
                    return BtResult::Failure;
                }
                if ctx.interface.summon_pet() {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::FeedPet => {
                if ctx.interface.feed_pet() {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::GrindTarget => tick_grind(ctx),

            // ── Actions — 11f ────────────────────────────────────────
            Bt::KiteFromTarget(dist) => tick_kite_from_target(ctx, *dist),
            Bt::CloseToTarget(dist) => tick_close_to_target(ctx, *dist),
            Bt::EngageTarget => tick_engage_target(ctx),
            Bt::PullTarget => tick_pull_target(ctx),

            // ── Actions — 11g ────────────────────────────────────────
            Bt::MarkRti(icon) | Bt::MarkRtiCc(icon) => {
                let target = match ctx.current_target() {
                    Some(t) => t,
                    None => return BtResult::Failure,
                };
                if *icon >= 8 {
                    return BtResult::Failure;
                }
                ok(ctx.interface.group_set_target_icon(target, *icon))
            }
            Bt::MarkRtiPreferred => {
                let icon = match ctx.settings.preferred_rti_icon {
                    Some(i) => i,
                    None => return BtResult::Failure,
                };
                let target = match ctx.current_target() {
                    Some(t) => t,
                    None => return BtResult::Failure,
                };
                ok(ctx.interface.group_set_target_icon(target, icon))
            }
            Bt::CcCastOnRti(spell) => tick_cc_cast_on_rti(ctx, *spell),
            Bt::CcCastOnNearest(spell) => tick_cc_cast_on_nearest(ctx, *spell),
            Bt::RtiAssist => tick_rti_assist(
                ctx,
                ctx.settings.preferred_rti_icon.unwrap_or(7),
            ),
            Bt::RtiCcTargetSelect => tick_rti_assist(
                ctx,
                ctx.settings.preferred_cc_rti_icon.unwrap_or(5),
            ),

            // ── Actions — Step 13: cross-class reactive combat ────────
            Bt::PullBack => tick_pull_back(ctx),
            Bt::WaitForAttack => tick_wait_for_attack(ctx),
            Bt::PreHeal => tick_preheal(ctx),
            Bt::HealInterrupt => tick_heal_interrupt(ctx),

            // ── Actions — 11h ────────────────────────────────────────
            Bt::UseBuffPotion => tick_use_potion(ctx, 0),
            Bt::UseUtilityPotion => tick_use_potion(ctx, 1),
            Bt::UseRacial(spell) => {
                if !ctx.interface.knows_spell(*spell) {
                    return BtResult::Failure;
                }
                cast(ctx, *spell, ctx.bot_handle)
            }
            Bt::UseTrinket(slot) => ok(ctx.interface.use_trinket(*slot)),

            // ── Actions — 11i ────────────────────────────────────────
            Bt::AcceptGroupInvite => ok(ctx.interface.accept_group_invite()),
            Bt::LeaveGroup => ok(ctx.interface.leave_group()),
            Bt::AcceptReadyCheck => ok(ctx.interface.accept_ready_check()),
            Bt::AcceptTradeRequest => ok(ctx.interface.accept_trade()),
            Bt::AcceptDuelRequest => ok(ctx.interface.accept_duel()),
            Bt::DeclineDuelRequest => ok(ctx.interface.decline_duel()),
            Bt::AcceptSummon => ok(ctx.interface.accept_summon()),
            Bt::UseMeetingStone => ok(ctx.interface.use_meeting_stone()),

            // ── World buffs ──────────────────────────────────────────
            Bt::ApplyWorldBuffs => tick_apply_world_buffs(ctx),
            Bt::WorldBuffTravel(spell) => tick_world_buff_travel(ctx, *spell),

            // ── Implemented actions ─────────────────────────────────
            Bt::ShareQuest => ok(ctx.interface.share_quest(0)),
            Bt::EquipItem(item) => ok(ctx.interface.equip_item(*item)),
            Bt::UnequipSlot(_slot) => {
                // UnequipSlot takes a slot index but the FFI takes item_id.
                // For now, return Failure — callers use EquipItem instead.
                BtResult::Failure
            }
            Bt::RandomEmote => tick_random_emote(ctx),
            Bt::RandomSay => tick_random_say(ctx),
            Bt::CaptureFlag => ok(ctx.interface.capture_bg_objective()),
            Bt::Gossip(entry) => tick_gossip(ctx, *entry),
            Bt::BuyFromVendor(item, qty) => tick_buy_from_vendor(ctx, *item, *qty),
            Bt::MailItem => tick_mail_item(ctx),
            Bt::BankDeposit => tick_bank_deposit(ctx),
            Bt::BankWithdraw => tick_bank_withdraw(ctx),
            Bt::AhPost => tick_ah_post(ctx),
            Bt::AhBid => tick_ah_bid(ctx),
            Bt::ApplyOutfit => tick_apply_outfit(ctx),
            Bt::Fish => tick_fish(ctx),
            Bt::RtscConsumeMoveQueue => tick_rtsc_consume_move_queue(ctx),
            Bt::LfgJoin => tick_lfg_join(ctx),
            Bt::LfgAccept => tick_lfg_accept(ctx),
            Bt::AcceptBgInvite => tick_accept_bg_invite(ctx),
            Bt::QueueBg => tick_queue_bg(ctx),
            Bt::DefendBase => tick_defend_base(ctx),
            Bt::ReturnFlag => tick_return_flag(ctx),
            Bt::AssaultBase => tick_assault_base(ctx),
            Bt::ArenaEngageSetup => tick_arena_engage_setup(ctx),
            Bt::ArenaPeel => tick_arena_peel(ctx),
            Bt::DungeonStayNearTank => tick_dungeon_stay_near_tank(ctx),
            Bt::DungeonAvoidBreakingCc => tick_dungeon_avoid_breaking_cc(ctx),
            Bt::DebugDumpState(kind) => tick_debug_dump_state(ctx, *kind),

            // ── Battleground ─────────────────────────────────────────────
            Bt::InBattleground => ok(ctx.interface.is_in_battleground()),
            Bt::BgCaptureObjective => tick_bg_capture(ctx),
            Bt::BgAttackEnemy => tick_bg_attack(ctx),

            // ── RPG ──────────────────────────────────────────────────────
            Bt::RpgWander => tick_rpg_wander(ctx),
            Bt::RpgInteractNpc => tick_rpg_interact(ctx),
            Bt::RpgEmote => tick_rpg_emote(ctx),

            // ── Gathering ────────────────────────────────────────────────
            Bt::HasGatheringSkill => ok(ctx.interface.has_gathering_skill()),
            Bt::GatherNode => tick_gather(ctx),

            // ── Noncombat ────────────────────────────────────────────────
            Bt::Consumables => tick_consumables(ctx),
            Bt::Buff(buffs) => tick_buff(ctx, buffs),
            Bt::EncounterOverride => match ctx.encounter.and_then(|e| e.phase_bt()) {
                Some(bt) => bt.tick(ctx),
                None => BtResult::Failure,
            }, // NOTE: `bt.tick(ctx)` above resolves to `Bt::tick` directly —
               // `ctx.encounter.phase_bt()` now returns `Option<&Bt>`, so there
               // is no vtable lookup at this call site.

            // ── Class preferences ─────────────────────────────────────────
            Bt::ApplyPoisons => crate::classes::rogue::poisons::tick_apply_poisons(ctx),
            Bt::DropConfiguredTotems => {
                crate::classes::shaman::totems::tick_drop_configured_totems(ctx)
            }
            Bt::ApplyShamanImbues => {
                crate::classes::shaman::imbues::tick_apply_shaman_imbues(ctx)
            }
            Bt::MaintainPaladinAura => {
                crate::classes::paladin::prefs::tick_maintain_paladin_aura(ctx)
            }
            Bt::ApplyConfiguredBlessings => {
                crate::classes::paladin::prefs::tick_apply_paladin_blessings(ctx)
            }
            Bt::MaintainHunterAspect => {
                crate::classes::hunter::prefs::tick_maintain_hunter_aspect(ctx)
            }
            Bt::MaintainConfiguredCurse => {
                crate::classes::warlock::prefs::tick_maintain_warlock_curse(ctx)
            }
            Bt::EnforceWarriorStance => {
                crate::classes::warrior::prefs::tick_enforce_warrior_stance(ctx)
            }

            // ── Instance / sub-area gating ───────────────────────────────
            Bt::InArea(id) => {
                if ctx.snap.area_id == *id {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::InAnyArea(ids) => {
                if ids.contains(&ctx.snap.area_id) {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::Custom(leaf) => (leaf.handler)(ctx),
        };
        // Record the winning leaf for the monitor trace.
        // Compositors (Seq/Sel/Not/Throttle) return above and push their own
        // path segments. Everything that reaches here is a leaf or condition.
        if let Some(ref trace) = ctx.monitor_trace {
            if matches!(result, BtResult::Success | BtResult::Running) {
                trace.borrow_mut().push(format!("{self:?}"));
            }
        }
        result
    }
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn ok(b: bool) -> BtResult {
    if b {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

/// Read `res` from the tick context and compare against `op`. Used by
/// [`Bt::Cmp`]. Power-type resources return 0 when the bot's primary power
/// is something else, which makes `Above(n)` safely fall through for classes
/// that don't use that resource (e.g. `Cmp(SelfEnergy, Above(60))` on a mage).
fn eval_cmp(ctx: &TickContext<'_>, res: Resource, op: Op) -> bool {
    let s = &ctx.snap.self_;
    // Integer percent 0..=100 from two u32s. Returns 0 when `max` is 0 so
    // comparisons behave predictably on dead / uninitialized units.
    let pct = |cur: u32, max: u32| -> u32 {
        if max == 0 {
            0
        } else {
            ((u64::from(cur) * 100) / u64::from(max)) as u32
        }
    };
    let val: u32 = match res {
        Resource::SelfHealth => s.health,
        Resource::SelfHealthPct => pct(s.health, s.max_health),
        Resource::SelfMana => {
            if s.power_type == 0 {
                s.mana
            } else {
                0
            }
        }
        Resource::SelfManaPct => {
            if s.power_type == 0 {
                pct(s.mana, s.max_mana)
            } else {
                0
            }
        }
        Resource::SelfRage => {
            if s.power_type == 1 {
                s.mana
            } else {
                0
            }
        }
        Resource::SelfEnergy => {
            if s.power_type == 3 {
                s.mana
            } else {
                0
            }
        }
        Resource::SelfRunicPower => {
            if s.power_type == 6 {
                s.mana
            } else {
                0
            }
        }
        Resource::SelfComboPoints => u32::from(s.combo_points),
        Resource::TargetHealth | Resource::TargetHealthPct => {
            let Some(t) = ctx.current_target() else {
                return false;
            };
            let ts = ctx.interface.get_unit_snapshot(t);
            match res {
                Resource::TargetHealth => ts.health,
                Resource::TargetHealthPct => pct(ts.health, ts.max_health),
                _ => unreachable!(),
            }
        }
        Resource::TargetDistance => ctx
            .current_target()
            .map_or(u32::MAX, |t| ctx.interface.unit_distance(t) as u32),
        Resource::NearbyCount => ctx.nearby.len() as u32,
        Resource::AttackerCount => ctx.attackers.len() as u32,
        Resource::GroupSize => u32::from(ctx.snap.group_size),
    };
    match op {
        Op::Above(n) => val > n,
        Op::Below(n) => val < n,
        Op::Exactly(n) => val == n,
        Op::AtLeast(n) => val >= n,
        Op::AtMost(n) => val <= n,
    }
}

fn move_to_safe(ctx: &mut TickContext<'_>, radius: f32) -> BtResult {
    match ctx.interface.get_safe_position(radius) {
        Some(pos) if ctx.interface.move_to(pos.x, pos.y, pos.z) => BtResult::Running,
        _ => BtResult::Failure,
    }
}

fn cast(ctx: &mut TickContext<'_>, spell: SpellId, target: u64) -> BtResult {
    // Don't interrupt an in-progress cast (e.g. Summon Demon, Hearthstone).
    if ctx.snap.self_.is_casting {
        let name = ctx.spell_name(spell);
        ctx.monitor(format_args!(
            "CAST {name} on 0x{target:X}: SKIP (already casting spell={})",
            ctx.snap.self_.casting_spell_id,
        ));
        return BtResult::Failure;
    }
    if ctx.timers.gcd_active(ctx.server_time_ms) {
        // GCD is too spammy for chat/monitor - only log when explicitly asked
        return BtResult::Failure;
    }
    if ctx.timers.spell_on_cooldown(spell, ctx.server_time_ms) {
        let rem = ctx.timers.cooldown_remaining_ms(spell, ctx.server_time_ms);
        let name = ctx.spell_name(spell);
        ctx.monitor(format_args!(
            "CAST {name} on 0x{target:X}: CD ({rem}ms left)",
        ));
        return BtResult::Failure;
    }
    if ctx.interface.cast_spell(spell, target) {
        ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
        let name = ctx.spell_name(spell);
        ctx.monitor(format_args!("CAST {name} on 0x{target:X}: OK"));
        BtResult::Success
    } else {
        let dist = ctx.interface.unit_distance(target);
        let moving = ctx.snap.self_.is_moving;
        let has_spell = ctx.interface.knows_spell(spell);
        let can = ctx.interface.can_cast(spell, target);
        let name = ctx.spell_name(spell);
        let los = ctx.interface.has_los(target);
        // Build a human-readable reason string.
        let reason = if !has_spell {
            "not learned"
        } else if !los {
            "no line of sight"
        } else if !can {
            "can't cast (range/mana/stance?)"
        } else {
            "server rejected"
        };
        ctx.monitor(format_args!(
            "CAST {name} on 0x{target:X}: FAIL ({reason}) dist={dist:.1}y los={los} knows={has_spell} can_cast={can} moving={moving}",
        ));
        BtResult::Failure
    }
}

fn count_group_members_below(ctx: &TickContext<'_>, threshold: f32) -> u8 {
    let mut count = 0u8;
    if ctx.self_hp_pct() < threshold {
        count += 1;
    }
    for i in 0..ctx.snap.group_size as usize {
        let h = ctx.snap.group_members[i];
        if h == 0 || h == ctx.bot_handle {
            continue;
        }
        let snap = ctx.interface.get_unit_snapshot(h);
        if snap.is_alive
            && snap.max_health > 0
            && (snap.health as f32 / snap.max_health as f32) < threshold
        {
            count += 1;
        }
    }
    count
}

fn check_setting(ctx: &TickContext<'_>, s: Setting) -> bool {
    match s {
        Setting::AutoMount => ctx.settings.auto_mount,
        Setting::AutoLoot => ctx.settings.auto_loot,
        Setting::AutoVendor => ctx.settings.auto_vendor,
        Setting::AutoRepair => ctx.settings.auto_repair,
        Setting::AutoAcceptQuest => ctx.settings.auto_accept_quest,
        Setting::AutoResurrect => ctx.settings.auto_resurrect,
        Setting::Verbose => ctx.settings.verbose,
    }
}

// ── Follow ──────────────────────────────────────────────────────────────────

/// Re-issue a chase/move command once the bot drifts farther than this
/// from its follow target. Matches PB2 `PlayerbotAIConfig::tooFarDistance`
/// — chase is sticky inside this radius, so we avoid thrashing the
/// movement generator on every tick. The **per-bot** follow distance
/// itself comes from `ctx.settings.follow_distance` (PB2 default 1.5,
/// overridable via the `follow <n>` chat command); the formation offset
/// on top of it is resolved via [`bot::formation::resolve_formation`].
const REFOLLOW_THRESHOLD: f32 = 8.0;

/// Humanoid collision-pad radius — PB2 `Unit::GetObjectBoundingRadius()`
/// returns ~0.389 for a player character. Used by the `near` formation
/// so tightly packed bots don't clip into the leader.
const HUMANOID_BOUNDING_RADIUS: f32 = 0.389;

/// PB2 `PlayerbotAIConfig::aiReactDistance` default (150y) — the range
/// within which a bot is considered "close enough to react" to its
/// master's situation. Mirrors `BotAiConfig::pb2_defaults().react_distance`.
/// Hardcoded until `BotAiConfig` is plumbed through `TickContext` (see
/// Part 5 Step 5 gotcha in `PB2_PARITY_PLAN.md`). When that lands, the
/// reader in `Bt::InReactRangeOfMaster` should switch to
/// `ctx.ai_config.react_distance`.
const DEFAULT_REACT_DISTANCE: f32 = 150.0;

/// Deterministic per-tick pseudo-random probability check for
/// [`Bt::RandomChance`]. Mixes `server_time_ms` with `bot_handle` via a
/// cheap xorshift-style splatter so two bots hitting the same
/// `RandomChance(p)` in the same tick get independent rolls. Not
/// cryptographically strong — fine for random emotes / jumps / jitter
/// gating, not for anything security-sensitive.
fn eval_random_chance(ctx: &TickContext<'_>, pct: u8) -> bool {
    if pct == 0 {
        return false;
    }
    if pct >= 100 {
        return true;
    }
    let mut seed = ctx
        .server_time_ms
        .wrapping_mul(0x9E37_79B1_7F4A_7C15)
        ^ ctx.bot_handle.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    // One round of xorshift to decorrelate low bits from the inputs.
    seed ^= seed >> 30;
    seed = seed.wrapping_mul(0x94D0_49BB_1331_11EB);
    seed ^= seed >> 27;
    ((seed >> 33) as u32 % 100) < u32::from(pct)
}

fn tick_follow(ctx: &mut TickContext<'_>) -> BtResult {
    // Follow-target priority order, mirroring PB2:
    //   1. The designated group tank.
    //   2. The recorded master (real player that claimed this bot).
    //   3. Any other group member.
    //
    // Result semantics — this is load-bearing for `mode_dispatch`:
    //
    //   * Success — a follow target was resolvable (we either kicked off a
    //     re-follow movement, or we're already close enough to stay put).
    //   * Failure — there is literally no follow target for this bot
    //     (solo, masterless random bot). The selector falls through to
    //     RPG/Grind so unclaimed bots still do something.

    let Some(target) = pick_follow_target(ctx) else {
        return BtResult::Failure;
    };

    // Don't interrupt a long cast (Summon Demon, Hearthstone, etc.) with
    // a movement command — the cast would be cancelled server-side.
    if ctx.snap.self_.is_casting {
        return BtResult::Success;
    }

    // For chase-offset formations (Melee, Queue, Far), the chase movement
    // generator maintains the offset once started. We only need to re-issue
    // when the bot drifts far from the target.
    //
    // For position-based formations (Near, Arrow, Raid, Line, Shield, etc.),
    // the bot must recompute its formation slot each tick as the leader moves.
    // The dedup in CB_MoveTo/CB_Follow prevents redundant commands when the
    // computed position hasn't changed, so we can safely call every tick.
    use crate::bot::settings::FollowFormation;
    let uses_chase = matches!(
        ctx.settings.follow_formation,
        FollowFormation::Melee | FollowFormation::Queue | FollowFormation::Far
    );

    let dist_to_target = ctx.interface.unit_distance(target);

    if uses_chase && dist_to_target <= REFOLLOW_THRESHOLD {
        // Chase generator is handling it — don't re-issue.
        return BtResult::Success;
    }

    // For position-based formations, throttle move_to re-issue when the bot
    // is close to the follow target. Without this, formation position shifts
    // every tick as the leader moves, causing MovePoint to fire every ~100ms
    // which restarts the spline and produces stuttering/gliding movement.
    // When close, only re-issue every ~1s; when far, always re-issue.
    if !uses_chase && dist_to_target <= REFOLLOW_THRESHOLD {
        use crate::engine::blackboard::{Key as BbKey, Value as BbValue};
        let last_follow_ms = ctx.blackboard.get_u64(BbKey::LastFollowMs).unwrap_or(0);
        if ctx.server_time_ms.saturating_sub(last_follow_ms) < 1_000 {
            return BtResult::Success;
        }
        ctx.blackboard.set(BbKey::LastFollowMs, BbValue::U64(ctx.server_time_ms));
    }

    apply_formation_follow(ctx, target);
    BtResult::Success
}

/// Pick the follow target handle per PB2's tank → master → peer order.
/// Excludes `bot_handle` (a bot never follows itself).
fn pick_follow_target(ctx: &TickContext<'_>) -> Option<UnitHandle> {
    if let Some(tank) = ctx.interface.group_get_tank()
        && tank != ctx.bot_handle
    {
        return Some(tank);
    }
    if let Some(master) = ctx.master_guid
        && master != 0
        && master != ctx.bot_handle
    {
        return Some(master);
    }
    ctx.snap.group_members[..ctx.snap.group_size as usize]
        .iter()
        .copied()
        .find(|&h| h != 0 && h != ctx.bot_handle)
}

/// Resolve the bot's formation slot against `target` and issue the
/// resulting movement command. Chase-offset formations go through
/// `interface.follow(target, offset, angle)`; position-based formations
/// go through `interface.move_to(x, y, z)`.
fn apply_formation_follow(ctx: &mut TickContext<'_>, target: UnitHandle) {
    let follow_range = ctx.settings.follow_distance;
    let formation = ctx.settings.follow_formation;

    // Pull the follow target's world position via an extra unit-snapshot
    // FFI call. Position-based formations need the target's `(x,y,z,o)`
    // to rotate offsets; chase-offset formations only need `o` for the
    // angle computation, but the call cost is identical.
    let target_unit = ctx.interface.get_unit_snapshot(target);
    let follow_target = target_unit.pos;

    // Build the group roster for formations that care (line, shield,
    // arrow, raid, near's `group_follow_angle`). Each member's role is
    // resolved via `group_get_role` — this is the only place in the
    // tick that walks the roster, so the cost is bounded.
    let members = &ctx.snap.group_members[..ctx.snap.group_size as usize];
    let mut roster: Vec<FormationMember> = Vec::with_capacity(members.len());
    for &h in members {
        if h == 0 || h == target {
            // Skip empty slots and the leader itself — PB2's roster walks
            // exclude the follow target, and `group_follow_angle` expects
            // the leader already stripped.
            continue;
        }
        let role = ctx.interface.group_get_role(h);
        roster.push(FormationMember {
            handle: h,
            is_tank: role.is_tank(),
            is_heal: role.is_heal(),
            // No dedicated "is alive" flag on `BotRole`; dead bots stop
            // following anyway (Dead state engine takes over). Treat
            // every roster entry as alive — mirrors PB2's behavior where
            // Formation::GetFollowAngle only walks living members via
            // `GroupReference::next()`.
            is_alive: true,
        });
    }

    // Load chaos state from blackboard so the jitter stays stable
    // within each 3-second window.
    let chaos_in = ChaosState {
        dx: ctx.blackboard.get_f32(BbKey::ChaosDx).unwrap_or(0.0),
        dy: ctx.blackboard.get_f32(BbKey::ChaosDy).unwrap_or(0.0),
        last_change_secs: ctx.blackboard.get_u64(BbKey::ChaosLastChangeSecs).unwrap_or(0),
    };

    let fctx = FormationContext {
        self_handle: ctx.bot_handle,
        follow_target,
        follow_range,
        bounding_radius: HUMANOID_BOUNDING_RADIUS,
        current_target: None,
        group: &roster,
        now_secs: ctx.server_time_ms / 1000,
        chaos_state: chaos_in,
        // `custom` formation offset plumbing lands when the
        // `setposition` command is ported (PB2 `SetPositionAction`).
        // Until then Custom falls back to `near`.
        custom_offset: None,
    };

    let result = resolve_formation(formation, &fctx);

    // Persist chaos state for the next tick.
    if result.chaos_state != chaos_in {
        ctx.blackboard
            .set(BbKey::ChaosDx, BbValue::F32(result.chaos_state.dx));
        ctx.blackboard
            .set(BbKey::ChaosDy, BbValue::F32(result.chaos_state.dy));
        ctx.blackboard.set(
            BbKey::ChaosLastChangeSecs,
            BbValue::U64(result.chaos_state.last_change_secs),
        );
    }

    match result.output {
        FormationOutput::ChaseOffset { offset, angle } => {
            ctx.interface.follow(target, offset, angle);
        }
        FormationOutput::Position { x, y, z: _ } => {
            // Convert absolute position to follow offset so we can use
            // MoveFollow (smooth target tracking) instead of MovePoint
            // (restarts spline every tick, causing stutter).
            let tx = follow_target.x;
            let ty = follow_target.y;
            let dx = x - tx;
            let dy = y - ty;
            let offset = dx.hypot(dy).max(0.5);
            let angle = dy.atan2(dx) - follow_target.o;
            ctx.interface.follow(target, offset, angle);
        }
    }
}

// ── Guard return ────────────────────────────────────────────────────────────

const GUARD_LEASH_DIST: f32 = 5.0;

fn tick_guard_return(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some((gx, gy, gz)) = ctx.settings.guard_position {
        let pos = &ctx.snap.self_.pos;
        let dx = pos.x - gx;
        let dy = pos.y - gy;
        let dist = dx.hypot(dy);
        if dist > GUARD_LEASH_DIST && ctx.interface.move_to(gx, gy, gz) {
            return BtResult::Running;
        }
    }
    BtResult::Failure
}

// ── Consumables ─────────────────────────────────────────────────────────────

const HP_EAT_THRESHOLD: f32 = 0.70;
const HP_FULL_THRESHOLD: f32 = 0.90;
const MANA_DRINK_THRESHOLD: f32 = 0.40;
const MANA_FULL_THRESHOLD: f32 = 0.80;

fn tick_consumables(ctx: &mut TickContext<'_>) -> BtResult {
    let uses_mana = ctx.snap.self_.power_type == 0;
    let hp_low = ctx.self_hp_pct() < HP_EAT_THRESHOLD;
    let mana_low = uses_mana && ctx.self_mana_pct() < MANA_DRINK_THRESHOLD;

    if !hp_low && !mana_low {
        return BtResult::Failure;
    }

    let hp_full = ctx.self_hp_pct() >= HP_FULL_THRESHOLD;
    let mana_full = !uses_mana || ctx.self_mana_pct() >= MANA_FULL_THRESHOLD;

    if hp_full && mana_full {
        return BtResult::Success;
    }

    ctx.interface.stop_moving();
    BtResult::Running
}

// ── Buffing ─────────────────────────────────────────────────────────────────

fn tick_buff(ctx: &mut TickContext<'_>, buffs: &[GroupBuff]) -> BtResult {
    for buff in buffs {
        if let Some(target_handle) = find_buff_target(ctx, buff)
            && ctx.interface.cast_spell(buff.spell_id, target_handle)
        {
            ctx.timers.on_spell_cast(buff.spell_id, ctx.server_time_ms);
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn find_buff_target(ctx: &mut TickContext<'_>, buff: &GroupBuff) -> Option<u64> {
    use crate::noncombat::buffing::BuffTarget;

    let me = ctx.bot_handle;
    match buff.target {
        BuffTarget::Me => {
            if !ctx.interface.has_aura(me, buff.aura_id) {
                Some(me)
            } else {
                None
            }
        }
        BuffTarget::Tank => ctx
            .interface
            .group_get_tank()
            .filter(|&t| !ctx.interface.has_aura(t, buff.aura_id)),
        BuffTarget::Healer => ctx
            .interface
            .group_get_healer()
            .filter(|&h| !ctx.interface.has_aura(h, buff.aura_id)),
        BuffTarget::AnyMember => std::iter::once(me)
            .chain(
                ctx.snap.group_members[..ctx.snap.group_size as usize]
                    .iter()
                    .copied()
                    .filter(|&h| h != 0 && h != me),
            )
            .find(|&h| !ctx.interface.has_aura(h, buff.aura_id)),
    }
}

// ── Reactive combat ─────────────────────────────────────────────────────────

// Interrupt spells per class
const KICK: SpellId = SpellId(1766);
const PUMMEL: SpellId = SpellId(6552);
const COUNTERSPELL: SpellId = SpellId(2139);
const EARTH_SHOCK: SpellId = SpellId(8042);
const FERAL_CHARGE: SpellId = SpellId(16979);

// Dispel spells per class
const DISPEL_MAGIC: SpellId = SpellId(988);
const CLEANSE: SpellId = SpellId(4987);
const CURE_POISON: SpellId = SpellId(526);
const REMOVE_CURSE: SpellId = SpellId(475);
const ABOLISH_POISON: SpellId = SpellId(2893);

// Resurrect spells per class
const RESURRECTION: SpellId = SpellId(2006);
const REDEMPTION: SpellId = SpellId(7328);
const REBIRTH: SpellId = SpellId(20484);
const ANCESTRAL_SPIRIT: SpellId = SpellId(2008);

// Threat management
const FADE: SpellId = SpellId(586);
const FEIGN_DEATH: SpellId = SpellId(5384);
const VANISH: SpellId = SpellId(1856);
const SOULSHATTER: SpellId = SpellId(29858);

fn tick_interrupt(ctx: &mut TickContext<'_>) -> BtResult {
    let target = match ctx.current_target() {
        Some(t) => t,
        None => return BtResult::Failure,
    };
    if !ctx.interface.is_casting_interruptible(target) {
        return BtResult::Failure;
    }
    let spell = match ctx.class {
        PlayerClass::Rogue => KICK,
        PlayerClass::Warrior => PUMMEL,
        PlayerClass::Mage => COUNTERSPELL,
        PlayerClass::Shaman => EARTH_SHOCK,
        PlayerClass::Druid => FERAL_CHARGE,
        _ => return BtResult::Failure,
    };
    if ctx.interface.can_cast(spell, target) && ctx.interface.cast_spell(spell, target) {
        ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
        let name = ctx.spell_name(spell);
        ctx.monitor(format_args!("REACTIVE: Interrupt {name} on 0x{target:X}"));
        BtResult::Success
    } else {
        let name = ctx.spell_name(spell);
        let can = ctx.interface.can_cast(spell, target);
        let dist = ctx.interface.unit_distance(target);
        ctx.monitor(format_args!(
            "REACTIVE: Interrupt {name} FAIL can_cast={can} dist={dist:.1}y on 0x{target:X}",
        ));
        BtResult::Failure
    }
}

fn tick_dispel(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some((member, debuff_id)) = ctx.interface.find_dispellable_target(DispelSchool::Any.mask()) {
        let spell = match ctx.class {
            PlayerClass::Priest => DISPEL_MAGIC,
            PlayerClass::Paladin => CLEANSE,
            PlayerClass::Druid => ABOLISH_POISON,
            PlayerClass::Mage => REMOVE_CURSE,
            PlayerClass::Shaman => CURE_POISON,
            _ => return BtResult::Failure,
        };
        if ctx.interface.can_cast(spell, member) && ctx.interface.cast_spell(spell, member) {
            ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
            let name = ctx.spell_name(spell);
            ctx.monitor(format_args!(
                "REACTIVE: Dispel {name} on 0x{member:X} (debuff={debuff_id})",
            ));
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn tick_resurrect(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some(dead) = ctx.interface.find_dead_party_member() {
        let spell = match ctx.class {
            PlayerClass::Priest => RESURRECTION,
            PlayerClass::Paladin => REDEMPTION,
            PlayerClass::Druid => REBIRTH,
            PlayerClass::Shaman => ANCESTRAL_SPIRIT,
            _ => return BtResult::Failure,
        };
        if spell != REBIRTH && ctx.in_combat() {
            return BtResult::Failure;
        }
        if ctx.interface.can_cast(spell, dead) && ctx.interface.cast_spell(spell, dead) {
            ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
            let name = ctx.spell_name(spell);
            ctx.monitor(format_args!("REACTIVE: Resurrect {name} on 0x{dead:X}"));
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn tick_pulling_aggro(ctx: &TickContext<'_>) -> bool {
    if let Some(target) = ctx.current_target() {
        let my_threat = ctx.interface.get_unit_threat(target, ctx.bot_handle);
        let threat_list = ctx.interface.get_threat_list(target);
        if let Some(top) = threat_list.first() {
            return my_threat > top.threat * 0.9 && top.unit != ctx.bot_handle;
        }
    }
    false
}

fn tick_threat_dump(ctx: &mut TickContext<'_>) -> BtResult {
    let spell = match ctx.class {
        PlayerClass::Priest => FADE,
        PlayerClass::Hunter => FEIGN_DEATH,
        PlayerClass::Rogue => VANISH,
        PlayerClass::Warlock => SOULSHATTER,
        _ => return BtResult::Failure,
    };
    let me = ctx.bot_handle;
    if ctx.interface.can_cast(spell, me) && ctx.interface.cast_spell(spell, me) {
        ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
        let name = ctx.spell_name(spell);
        ctx.monitor(format_args!("REACTIVE: ThreatDump {name}"));
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_focus_attack(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some(focus) = ctx.settings.focus_target {
        if ctx.current_target() == Some(focus) {
            ctx.pending_target.set(Some(focus));
            return BtResult::Success;
        }
        if ctx.attack(focus) {
            ctx.monitor(format_args!("TARGET: FocusAttack -> 0x{focus:X}"));
            return BtResult::Success;
        }
        ctx.monitor(format_args!(
            "TARGET: FocusAttack -> attack(0x{focus:X}) REFUSED",
        ));
    }
    BtResult::Failure
}

fn tick_tank_pickup(ctx: &mut TickContext<'_>) -> BtResult {
    for &attacker in ctx.attackers {
        let snap = ctx.interface.get_unit_snapshot(attacker);
        if snap.current_target != ctx.bot_handle {
            if ctx.interface.taunt(attacker) {
                ctx.monitor(format_args!(
                    "TARGET: TankPickup -> taunt 0x{attacker:X} (was on 0x{:X})",
                    snap.current_target,
                ));
                return BtResult::Success;
            }
            ctx.monitor(format_args!(
                "TARGET: TankPickup -> taunt(0x{attacker:X}) REFUSED",
            ));
        }
    }
    BtResult::Failure
}

fn tick_assist_leader(ctx: &mut TickContext<'_>) -> BtResult {
    let tank = ctx.interface.group_get_tank();
    let leader = tank.or_else(|| {
        ctx.snap.group_members[..ctx.snap.group_size as usize]
            .iter()
            .copied()
            .find(|&h| h != 0 && h != ctx.bot_handle)
    });

    let leader_target = leader.and_then(|l| {
        let snap = ctx.interface.get_unit_snapshot(l);
        if snap.current_target != 0 {
            Some(snap.current_target)
        } else {
            None
        }
    });

    if let Some(target) = leader_target {
        if ctx.current_target() == Some(target) {
            ctx.pending_target.set(Some(target));
            return BtResult::Success;
        }
        if ctx.attack(target) {
            ctx.monitor(format_args!(
                "TARGET: AssistLeader -> attack 0x{:X} (leader=0x{:X})",
                target,
                leader.unwrap_or(0),
            ));
            return BtResult::Success;
        }
        ctx.monitor(format_args!(
            "TARGET: AssistLeader -> attack(0x{:X}) REFUSED by FFI",
            target,
        ));
    } else {
        ctx.monitor(format_args!(
            "TARGET: AssistLeader FAIL tank={:?} leader={:?} group_size={} (leader has no target)",
            tank.map(|t| format!("0x{t:X}")),
            leader.map(|l| format!("0x{l:X}")),
            ctx.snap.group_size,
        ));
    }
    BtResult::Failure
}

fn tick_protect(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some(protect) = ctx.settings.protect_target {
        let attacker = ctx.attackers.iter().copied().find(|&a| {
            ctx.interface.get_unit_snapshot(a).current_target == protect
        });
        if let Some(target) = attacker {
            if ctx.current_target() == Some(target) {
                ctx.pending_target.set(Some(target));
                return BtResult::Success;
            }
            if ctx.attack(target) {
                ctx.monitor(format_args!(
                    "TARGET: Protect -> attack 0x{target:X} (protecting 0x{protect:X})",
                ));
                return BtResult::Success;
            }
        }
    }
    BtResult::Failure
}

// ── World action helpers ────────────────────────────────────────────────────

/// NPC flag bitmask for vendors.
const NPC_FLAG_VENDOR: u32 = 0x80;
/// NPC flag bitmask for repair-capable NPCs.
const NPC_FLAG_REPAIR: u32 = 0x1000;
/// NPC flag for quest givers.
const NPC_FLAG_QUESTGIVER: u32 = 0x02;

fn tick_loot(ctx: &mut TickContext<'_>) -> BtResult {
    let lootable = ctx.interface.get_nearby_lootable(15.0);
    if let Some(&corpse) = lootable.first() {
        let dist = ctx.interface.unit_distance(corpse);
        if dist > 5.0 {
            let snap = ctx.interface.get_unit_snapshot(corpse);
            if ctx.interface.move_to(snap.pos.x, snap.pos.y, snap.pos.z) {
                return BtResult::Running;
            }
        } else if ctx.interface.open_loot(corpse) {
            ctx.interface.take_all_loot();
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

/// `Bt::LootRoll`. Vote on the next pending loot roll using the bot's
/// auto-determined vote (need/greed/pass).
fn tick_loot_roll(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.get_pending_roll_count() == 0 {
        return BtResult::Failure;
    }
    if ctx.interface.auto_loot_roll() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

/// `Bt::AutoLootRoll`. Same as LootRoll but intended for the delayed-roll
/// strategy (rolls after a short delay).
fn tick_auto_loot_roll(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.get_pending_roll_count() == 0 {
        return BtResult::Failure;
    }
    if ctx.interface.auto_loot_roll() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_check_mail(ctx: &mut TickContext<'_>) -> BtResult {
    let summary = ctx.interface.bot_mail_summary();
    if summary.total_mails == 0 {
        return BtResult::Failure;
    }
    // bot_mail_take_all checks mailbox proximity internally
    if ctx.interface.bot_mail_take_all() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_learn_trainer_spells(ctx: &mut TickContext<'_>) -> BtResult {
    // Learn all class-appropriate spells for the bot's level, including quest rewards.
    ctx.interface.bot_learn_class_level_spells(true);
    BtResult::Success
}

fn tick_apply_talent_build(ctx: &mut TickContext<'_>) -> BtResult {
    let free_points = ctx.interface.bot_free_talent_points();
    if free_points == 0 {
        return BtResult::Failure;
    }
    // Pick the bot's preferred spec tab (0..2).
    let spec = ctx.interface.bot_pick_spec_no(true) as u8;
    let talents = ctx.interface.get_class_talents(spec);
    if talents.is_empty() {
        return BtResult::Failure;
    }
    // Learn talent ranks in row order, spending one point at a time.
    let mut spent = false;
    for talent in &talents {
        for &rank_spell in &talent.rank_ids {
            if rank_spell == 0 {
                break;
            }
            if ctx.interface.bot_free_talent_points() == 0 {
                break;
            }
            ctx.interface.bot_learn_spell(rank_spell);
            ctx.interface.bot_update_free_talent_points();
            spent = true;
        }
    }
    if spent { BtResult::Success } else { BtResult::Failure }
}

fn tick_vendor(ctx: &mut TickContext<'_>) -> BtResult {
    let npcs = ctx.interface.get_nearby_npcs(30.0, NPC_FLAG_VENDOR);
    if let Some(&vendor) = npcs.first() {
        return approach_and_interact(ctx, vendor, |ctx| {
            ctx.interface.sell_grey_items();
            BtResult::Success
        });
    }
    BtResult::Failure
}

fn tick_repair(ctx: &mut TickContext<'_>) -> BtResult {
    let npcs = ctx.interface.get_nearby_npcs(30.0, NPC_FLAG_REPAIR);
    if let Some(&vendor) = npcs.first() {
        return approach_and_interact(ctx, vendor, |ctx| {
            ctx.interface.repair_all();
            BtResult::Success
        });
    }
    BtResult::Failure
}

fn tick_turn_in_quest(ctx: &mut TickContext<'_>) -> BtResult {
    let quests = ctx.interface.get_quest_log();
    let completed = quests.iter().find(|q| q.complete);
    if let Some(quest) = completed {
        let quest_id = quest.quest_id;
        let npcs = ctx.interface.get_nearby_npcs(30.0, NPC_FLAG_QUESTGIVER);
        if let Some(&npc) = npcs.first() {
            return approach_and_interact(ctx, npc, |ctx| {
                if ctx.interface.turn_in_quest(npc, quest_id) {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            });
        }
    }
    BtResult::Failure
}

fn tick_accept_quests(ctx: &mut TickContext<'_>) -> BtResult {
    let npcs = ctx.interface.get_nearby_npcs(15.0, NPC_FLAG_QUESTGIVER);
    if let Some(&npc) = npcs.first() {
        return approach_and_interact(ctx, npc, |ctx| {
            if ctx.interface.accept_all_quests(npc) {
                BtResult::Success
            } else {
                BtResult::Failure
            }
        });
    }
    BtResult::Failure
}

fn tick_attack_quest_mob(ctx: &mut TickContext<'_>) -> BtResult {
    let quests = ctx.interface.get_quest_log();
    let has_active = quests.iter().any(|q| !q.complete);
    if has_active
        && let Some(&target) = ctx.nearby.iter().find(|&&u| ctx.interface.is_attackable(u))
        && ctx.attack(target)
    {
        return BtResult::Success;
    }
    BtResult::Failure
}

fn tick_travel(ctx: &mut TickContext<'_>) -> BtResult {
    use crate::engine::blackboard::Key;

    let dx = ctx.blackboard.get_f32(Key::TravelDestX).unwrap_or(0.0);
    let dy = ctx.blackboard.get_f32(Key::TravelDestY).unwrap_or(0.0);
    let dz = ctx.blackboard.get_f32(Key::TravelDestZ).unwrap_or(0.0);

    let pos = &ctx.snap.self_.pos;
    let dist_sq = (pos.x - dx).powi(2) + (pos.y - dy).powi(2);

    if dist_sq < 25.0 {
        ctx.blackboard.clear(Key::TravelDestX);
        ctx.blackboard.clear(Key::TravelDestY);
        ctx.blackboard.clear(Key::TravelDestZ);
        return BtResult::Success;
    }

    if ctx.interface.move_to(dx, dy, dz) {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

/// `Bt::ChooseTravelTarget`. Evaluate the bot's needs (repair, vendor,
/// quest objectives, grind) and pick the best reachable destination.
/// Writes the chosen destination to the blackboard so `TravelToBlackboard`
/// can navigate there.
///
/// Port of PB2's `TravelStrategy::InitNonCombatTriggers` destination
/// priority table + `ChooseTravelTargetAction`.
fn tick_choose_travel_target(ctx: &mut TickContext<'_>) -> BtResult {
    use crate::engine::blackboard::Key;
    use crate::travel::destination::{TravelDestination, TravelKind, TravelPurpose};

    // Don't overwrite an existing travel destination.
    if ctx.blackboard.get_f32(Key::TravelDestX).is_some() {
        return BtResult::Failure;
    }

    // Don't pick travel targets in combat.
    if ctx.in_combat() {
        return BtResult::Failure;
    }

    // Evaluate needs — PB2 TravelStrategy priority table.
    let durability = ctx.interface.get_durability_pct();
    let has_sellable = ctx.interface.has_sellable_items();
    let quest_log = ctx.interface.get_quest_log();
    let free_quest_slots = 25u8.saturating_sub(quest_log.len() as u8);
    let has_active_quests = !quest_log.is_empty();
    let level = ctx.snap.self_.level;

    let needs = crate::travel::planner::evaluate_needs(
        durability,
        has_sellable,
        free_quest_slots,
        has_active_quests,
        level,
    );

    // Try each need in priority order — query FFI for destinations.
    for (purpose, _relevance) in &needs {
        let dests = ctx
            .interface
            .find_travel_dests(purpose.bits(), 1000.0, 5);
        if dests.is_empty() {
            continue;
        }

        // Pick the nearest one on the same map.
        let current_map = ctx.snap.self_.pos.map_id;
        let pos = &ctx.snap.self_.pos;
        if let Some(best) = dests
            .iter()
            .filter(|d| d.map_id == current_map)
            .min_by(|a, b| {
                let da = (a.x - pos.x).powi(2) + (a.y - pos.y).powi(2);
                let db = (b.x - pos.x).powi(2) + (b.y - pos.y).powi(2);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            let kind = match *purpose {
                TravelPurpose::VENDOR => TravelKind::Vendor,
                TravelPurpose::REPAIR => TravelKind::Vendor,
                TravelPurpose::TRAINER => TravelKind::Trainer,
                TravelPurpose::QUEST_GIVER => TravelKind::QuestRelation,
                TravelPurpose::QUEST_TAKER => TravelKind::QuestRelation,
                TravelPurpose::GRIND => TravelKind::GrindSpot,
                TravelPurpose::EXPLORE => TravelKind::Explore,
                TravelPurpose::GENERIC_RPG => TravelKind::Rpg,
                TravelPurpose::AH => TravelKind::AuctionHouse,
                _ => TravelKind::NamedLocation,
            };
            let dest = TravelDestination::new(
                kind,
                *purpose,
                best.map_id,
                best.x,
                best.y,
                best.z,
            )
            .with_entry(best.entry);

            crate::travel::planner::set_travel_dest(ctx.blackboard, &dest);
            return BtResult::Success;
        }
    }

    BtResult::Failure
}

/// `Bt::ApplyWorldBuffs`. Get the list of missing world buffs from the C++
/// config system and directly apply each one via AddAura.
fn tick_apply_world_buffs(ctx: &mut TickContext<'_>) -> BtResult {
    let needed = ctx.interface.get_needed_world_buffs();
    if needed.is_empty() {
        return BtResult::Failure;
    }
    for spell_id in &needed {
        ctx.interface.add_aura(*spell_id);
    }
    BtResult::Success
}

/// `Bt::WorldBuffTravel`. Look up the world buff location for `spell`
/// and write it to the blackboard so `TravelToBlackboard` navigates there.
fn tick_world_buff_travel(ctx: &mut TickContext<'_>, spell: SpellId) -> BtResult {
    use crate::engine::blackboard::Key;
    use crate::travel::world_buff;

    // If the bot already has this buff, no need to travel.
    if ctx.interface.has_aura(ctx.bot_handle, spell) {
        return BtResult::Failure;
    }

    // Already have a travel destination set — don't overwrite.
    if ctx.blackboard.get_f32(Key::TravelDestX).is_some() {
        return BtResult::Failure;
    }

    let current_map = ctx.snap.self_.pos.map_id;
    let dest = match world_buff::find_world_buff_location(spell, current_map) {
        Some(d) => d,
        None => return BtResult::Failure,
    };

    crate::travel::planner::set_travel_dest(ctx.blackboard, &dest);
    BtResult::Success
}

// ── Implemented action tick handlers ─────────────────────────────────────────

fn tick_random_emote(ctx: &mut TickContext<'_>) -> BtResult {
    // Common emote IDs: wave=1, bow=2, dance=4, laugh=11, cheer=21
    let emotes = [1u32, 2, 4, 11, 21, 22, 23, 24, 41, 101];
    let idx = (ctx.snap.server_time_ms as usize) % emotes.len();
    if ctx.interface.do_text_emote(emotes[idx]) {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_random_say(ctx: &mut TickContext<'_>) -> BtResult {
    let phrases = [
        "Ready to go!",
        "Let's do this.",
        "On it.",
        "Following.",
        "Standing by.",
    ];
    let idx = (ctx.snap.server_time_ms as usize) % phrases.len();
    if ctx.interface.say(phrases[idx], 0) {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_gossip(ctx: &mut TickContext<'_>, entry: u32) -> BtResult {
    if ctx.interface.gossip_hello(entry) {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_buy_from_vendor(ctx: &mut TickContext<'_>, item: ItemId, qty: u32) -> BtResult {
    if ctx.interface.buy_from_vendor(item.raw(), qty) {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_mail_item(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.mail_item_to_master() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_bank_deposit(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.bank_deposit() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_bank_withdraw(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.bank_withdraw() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_ah_post(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.ah_post() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_ah_bid(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.ah_bid() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_apply_outfit(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.apply_outfit() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_fish(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.start_fishing() {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

fn tick_rtsc_consume_move_queue(ctx: &mut TickContext<'_>) -> BtResult {
    use crate::engine::blackboard::Key;
    // Consume the next RTSC move waypoint from the blackboard
    if let (Some(x), Some(y), Some(z)) = (
        ctx.blackboard.get_f32(Key::RtscMoveX),
        ctx.blackboard.get_f32(Key::RtscMoveY),
        ctx.blackboard.get_f32(Key::RtscMoveZ),
    ) {
        ctx.blackboard.clear(Key::RtscMoveX);
        ctx.blackboard.clear(Key::RtscMoveY);
        ctx.blackboard.clear(Key::RtscMoveZ);
        if ctx.interface.move_to(x, y, z) {
            BtResult::Running
        } else {
            BtResult::Failure
        }
    } else {
        BtResult::Failure
    }
}

fn tick_lfg_join(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.lfg_join() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_lfg_accept(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.lfg_accept() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_accept_bg_invite(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.accept_bg_invite() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_queue_bg(ctx: &mut TickContext<'_>) -> BtResult {
    if ctx.interface.queue_bg() {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_defend_base(ctx: &mut TickContext<'_>) -> BtResult {
    let pos = ctx.interface.get_bg_objective_pos(0); // 0 = defend
    if pos.x == 0.0 && pos.y == 0.0 && pos.z == 0.0 {
        return BtResult::Failure;
    }
    if ctx.interface.move_to(pos.x, pos.y, pos.z) {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

fn tick_return_flag(ctx: &mut TickContext<'_>) -> BtResult {
    let pos = ctx.interface.get_bg_objective_pos(3); // 3 = return_flag
    if pos.x == 0.0 && pos.y == 0.0 && pos.z == 0.0 {
        return BtResult::Failure;
    }
    if ctx.interface.move_to(pos.x, pos.y, pos.z) {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

fn tick_assault_base(ctx: &mut TickContext<'_>) -> BtResult {
    let pos = ctx.interface.get_bg_objective_pos(1); // 1 = assault
    if pos.x == 0.0 && pos.y == 0.0 && pos.z == 0.0 {
        return BtResult::Failure;
    }
    if ctx.interface.move_to(pos.x, pos.y, pos.z) {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

fn tick_arena_engage_setup(ctx: &mut TickContext<'_>) -> BtResult {
    // Move toward the center of the arena to engage
    let pos = ctx.interface.get_bg_objective_pos(1); // Use assault position
    if pos.x == 0.0 && pos.y == 0.0 && pos.z == 0.0 {
        return BtResult::Failure;
    }
    if ctx.interface.move_to(pos.x, pos.y, pos.z) {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

fn tick_arena_peel(ctx: &mut TickContext<'_>) -> BtResult {
    // Move toward the healer/teammate under pressure
    let pos = ctx.interface.get_tank_position();
    if pos.x == 0.0 && pos.y == 0.0 && pos.z == 0.0 {
        return BtResult::Failure;
    }
    // Move to defend position near the tank/healer
    if ctx.interface.move_to(pos.x, pos.y, pos.z) {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

fn tick_dungeon_stay_near_tank(ctx: &mut TickContext<'_>) -> BtResult {
    let tank_pos = ctx.interface.get_tank_position();
    if tank_pos.x == 0.0 && tank_pos.y == 0.0 && tank_pos.z == 0.0 {
        return BtResult::Failure;
    }

    let self_pos = &ctx.snap.self_.pos;
    let dx = self_pos.x - tank_pos.x;
    let dy = self_pos.y - tank_pos.y;
    let dist_sq = dx * dx + dy * dy;

    // Stay within 15 yards of the tank
    if dist_sq > 15.0 * 15.0 {
        if ctx.interface.move_to(tank_pos.x, tank_pos.y, tank_pos.z) {
            return BtResult::Running;
        }
    }

    BtResult::Success
}

fn tick_dungeon_avoid_breaking_cc(ctx: &mut TickContext<'_>) -> BtResult {
    // Check nearby enemies for CC — if our current target is CC'd, switch targets
    if let Some(target) = ctx.current_target() {
        if ctx.interface.is_unit_cc(target) {
            // Target is CC'd — stop attacking it
            ctx.interface.auto_attack(false);
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn tick_debug_dump_state(ctx: &mut TickContext<'_>, kind: u8) -> BtResult {
    if ctx.interface.debug_dump_state(kind) {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_grind(ctx: &mut TickContext<'_>) -> BtResult {
    // Re-engage existing target.
    if let Some(t) = ctx.current_target()
        && ctx.interface.is_attackable(t)
        && ctx.attack(t)
    {
        return BtResult::Success;
    }
    // Find a level-appropriate attackable mob.
    let my_level = ctx.snap.self_.level;
    let target = ctx.nearby.iter().copied().find(|&unit| {
        if !ctx.interface.is_attackable(unit) {
            return false;
        }
        let level = ctx.interface.get_unit_level(unit);
        let diff = (level as i16 - my_level as i16).unsigned_abs();
        diff <= 3 && ctx.interface.unit_distance(unit) < ctx.settings.max_combat_range
    });
    if let Some(t) = target
        && ctx.attack(t)
    {
        return BtResult::Success;
    }
    BtResult::Failure
}

/// Approach an NPC and interact when close enough.
fn approach_and_interact(
    ctx: &mut TickContext<'_>,
    npc: u64,
    on_interact: impl FnOnce(&mut TickContext<'_>) -> BtResult,
) -> BtResult {
    let dist = ctx.interface.unit_distance(npc);
    if dist > 5.0 {
        let snap = ctx.interface.get_unit_snapshot(npc);
        if ctx.interface.move_to(snap.pos.x, snap.pos.y, snap.pos.z) {
            return BtResult::Running;
        }
    } else if ctx.interface.interact_npc(npc) {
        return on_interact(ctx);
    }
    BtResult::Failure
}

// ── 11f: generic movement / positioning helpers ────────────────────────────

/// Move directly away from the current target along the target→bot
/// line until the bot sits at `dist * 2` yards from the target. Used by
/// `Bt::KiteFromTarget`. Returns `Failure` when the bot has no target,
/// is already outside `dist`, the horizontal line length collapses to
/// zero (the bot is standing exactly on the target — movement would
/// pick an undefined direction), or the move request is refused by the
/// interface. Returns `Running` on success.
fn tick_kite_from_target(ctx: &mut TickContext<'_>, dist: f32) -> BtResult {
    let target = match ctx.current_target() {
        Some(t) => t,
        None => return BtResult::Failure,
    };
    if ctx.interface.unit_distance(target) >= dist {
        return BtResult::Failure;
    }
    let tgt = ctx.interface.get_unit_snapshot(target);
    let bot = ctx.snap.self_.pos;
    let dx = bot.x - tgt.pos.x;
    let dy = bot.y - tgt.pos.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 0.000_1 {
        // Bot is essentially on top of the target — we have no direction
        // to kite in. Strategies that care should pair this with
        // `AttackerCountAbove(0)` / a melee-range check first.
        return BtResult::Failure;
    }
    let inv = (dist * 2.0) / len_sq.sqrt();
    let destx = tgt.pos.x + dx * inv;
    let desty = tgt.pos.y + dy * inv;
    if ctx.interface.move_to(destx, desty, bot.z) {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

/// Chase the current target when outside `dist` yards. Used by
/// `Bt::CloseToTarget`. Semantically equivalent to `StickToTarget`
/// but kept as its own helper so future divergence (e.g. a "close
/// with leap" variant for classes with a gap closer) lands cleanly.
fn tick_close_to_target(ctx: &mut TickContext<'_>, dist: f32) -> BtResult {
    if ctx.snap.self_.is_casting {
        return BtResult::Failure;
    }
    let target = match ctx.current_target() {
        Some(t) => t,
        None => return BtResult::Failure,
    };
    let cur_dist = ctx.interface.unit_distance(target);
    if cur_dist <= dist {
        return BtResult::Failure;
    }
    ctx.monitor(format_args!(
        "MOVE: CloseToTarget({dist}) cur={cur_dist:.1}y -> chase 0x{target:X}",
    ));
    if ctx.interface.chase(target, dist, 0.0) {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

/// Dispatch a pull on the current target across the three generic
/// paths (auto-shoot → taunt → attack) in priority order. Used by
/// `Bt::PullTarget`. Returns `Success` on the first path that reports
/// success; `Failure` when every path bails (no target, no ranged
/// weapon + no taunt + attack refused, etc.). Class-specific pulls
/// wrap this leaf; they are never inlined here.
/// Ensure auto-attack / auto-shoot is engaged on the current target.
/// For ranged-equipped bots, tries `auto_shoot` first (Auto Shot for
/// bow/gun/crossbow, Shoot for wand). Falls back to melee `auto_attack`.
/// Always returns Success so the combat Seq continues.
fn tick_engage_target(ctx: &mut TickContext<'_>) -> BtResult {
    let Some(target) = ctx.current_target() else {
        ctx.monitor(format_args!("ENGAGE: no target — pass-through"));
        return BtResult::Success; // no target yet — don't block
    };
    // Try ranged auto-shoot first (idempotent if already firing).
    if ctx.interface.auto_shoot(target) {
        ctx.monitor(format_args!("ENGAGE: auto_shoot on 0x{target:X}"));
    } else {
        // No ranged weapon or shoot failed — ensure melee auto-attack.
        ctx.interface.auto_attack(true);
        ctx.monitor(format_args!("ENGAGE: auto_attack on 0x{target:X}"));
    }
    BtResult::Success
}

fn tick_pull_target(ctx: &mut TickContext<'_>) -> BtResult {
    let target = match ctx.current_target() {
        Some(t) => t,
        None => return BtResult::Failure,
    };
    if ctx.interface.auto_shoot(target) {
        return BtResult::Success;
    }
    if ctx.interface.taunt(target) {
        return BtResult::Success;
    }
    if ctx.attack(target) {
        return BtResult::Success;
    }
    BtResult::Failure
}

// ── Step 13: cross-class reactive combat helpers ──────────────────────────

/// `Bt::PullBack`. After pulling a mob, return to the group's position.
/// Uses `follow(master)` to move back toward the master/leader. Returns
/// Running while the bot is far from the master, Success once close
/// enough, Failure when there is no master to return to.
fn tick_pull_back(ctx: &mut TickContext<'_>) -> BtResult {
    // Pull back to the master/tank/group member, reusing the same
    // priority order as the Follow leaf.
    let target = match pick_follow_target(ctx) {
        Some(t) => t,
        None => return BtResult::Failure,
    };
    let dist = ctx.interface.unit_distance(target);
    if dist <= 5.0 {
        return BtResult::Success;
    }
    if ctx.interface.follow(target, 2.0, 0.0) {
        BtResult::Running
    } else {
        BtResult::Failure
    }
}

/// `Bt::WaitForAttack`. Pauses until an attacker is within melee range
/// (≤ 8 yards). Used after a pull-back so the tank waits for the mob
/// to arrive instead of running back out to meet it.
fn tick_wait_for_attack(ctx: &mut TickContext<'_>) -> BtResult {
    let target = match ctx.current_target() {
        Some(t) => t,
        None => return BtResult::Failure,
    };
    let dist = ctx.interface.unit_distance(target);
    if dist <= 8.0 {
        BtResult::Success
    } else {
        BtResult::Running
    }
}

/// `Bt::PreHeal`. Cross-class preheal fallback — returns Failure.
/// Actual preheal logic is class-specific (each healer spec knows its
/// fast heal spell) and is layered via the class file's reaction
/// leaves. This variant exists so strategies can reference it
/// generically; class files override by inserting a `HealLowest`
/// or `CastOnLowestAlly` leaf at higher priority.
fn tick_preheal(_ctx: &mut TickContext<'_>) -> BtResult {
    BtResult::Failure
}

/// `Bt::HealInterrupt`. Cancel the bot's own cast if the heal target
/// is no longer injured (overheal prevention). Uses the
/// `interrupt_own_cast` FFI callback.
fn tick_heal_interrupt(ctx: &mut TickContext<'_>) -> BtResult {
    // Only relevant if the bot is currently casting
    if !ctx.snap.self_.is_casting {
        return BtResult::Failure;
    }

    // Check if the target we're healing is still injured
    if let Some(target) = ctx.current_target() {
        let snap = ctx.interface.get_unit_snapshot(target);
        if snap.max_health > 0 {
            let pct = (snap.health as f32) / (snap.max_health as f32);
            // If the target is above 95% health, cancel the heal
            if pct > 0.95 {
                if ctx.interface.interrupt_own_cast() {
                    return BtResult::Success;
                }
            }
        }
    }

    BtResult::Failure
}

// ── 11g: RTI / CC targeting helpers ────────────────────────────────────────

/// Resolve the unit currently wearing icon `icon_0to7` and cast `spell`
/// on it. Used by `Bt::CcCastOnRti`. The `get_unit_with_raid_icon` FFI
/// uses 1..=8 indexing while the settings use 0..=7, so the caller
/// passes the 0..=7 form and we translate here.
fn tick_cc_cast_on_rti(ctx: &mut TickContext<'_>, spell: SpellId) -> BtResult {
    let icon = ctx.settings.preferred_cc_rti_icon.unwrap_or(5);
    if icon >= 8 {
        return BtResult::Failure;
    }
    let unit = match ctx.interface.get_unit_with_raid_icon(icon + 1) {
        Some(u) => u,
        None => return BtResult::Failure,
    };
    if !ctx.interface.can_cast(spell, unit) {
        return BtResult::Failure;
    }
    cast(ctx, spell, unit)
}

/// Cast `spell` on the nearest attackable unit that is NOT the current
/// target and does NOT already carry `spell`. Mirrors
/// `Bt::CastCrowdControl` — kept as its own helper so future divergence
/// (line-of-sight, facing, aura-stack caps) doesn't disturb the
/// existing reactive code path.
fn tick_cc_cast_on_nearest(ctx: &mut TickContext<'_>, spell: SpellId) -> BtResult {
    let current = ctx.current_target().unwrap_or(0);
    let victim = ctx.nearby.iter().copied().find(|&u| {
        u != current
            && !ctx.interface.has_aura(u, spell)
            && ctx.interface.can_cast(spell, u)
    });
    match victim {
        Some(t) => cast(ctx, spell, t),
        None => BtResult::Failure,
    }
}

/// Switch the bot's focus to the unit wearing `icon_0to7` and start
/// attacking it. Used by both `Bt::RtiAssist` and
/// `Bt::RtiCcTargetSelect` (they differ only in which icon they read
/// from `BotSettings`). Converts the 0..=7 settings icon to the 1..=8
/// `get_unit_with_raid_icon` indexing at the boundary.
fn tick_rti_assist(ctx: &mut TickContext<'_>, icon_0to7: u8) -> BtResult {
    if icon_0to7 >= 8 {
        return BtResult::Failure;
    }
    match ctx.interface.get_unit_with_raid_icon(icon_0to7 + 1) {
        Some(u) if ctx.attack(u) => BtResult::Success,
        _ => BtResult::Failure,
    }
}

// ── 11h: consumable / potion helper ────────────────────────────────────────

/// Look up a potion from bags by `category` (0 = buff, 1 = utility),
/// gate on the shared potion cooldown, and use it on self. Used by
/// `Bt::UseBuffPotion` and `Bt::UseUtilityPotion`.
fn tick_use_potion(ctx: &mut TickContext<'_>, category: u8) -> BtResult {
    let item = ctx.interface.find_potion_in_bags(category);
    if item.0 == 0 {
        return BtResult::Failure;
    }
    if !ctx.interface.potion_cooldown_ready() {
        return BtResult::Failure;
    }
    ok(ctx.interface.use_item(item, ctx.bot_handle))
}

// ── Battleground helpers ────────────────────────────────────────────────────

fn tick_bg_capture(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some(pos) = ctx.interface.get_bg_objective() {
        let self_pos = &ctx.snap.self_.pos;
        let dist_sq = (self_pos.x - pos.x).powi(2) + (self_pos.y - pos.y).powi(2);
        if dist_sq > 25.0 {
            if ctx.interface.move_to(pos.x, pos.y, pos.z) {
                return BtResult::Running;
            }
        } else if ctx.interface.capture_bg_objective() {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn tick_bg_attack(ctx: &mut TickContext<'_>) -> BtResult {
    let enemies = ctx.interface.get_nearby_enemies(30.0);
    if let Some(&enemy) = enemies.first()
        && ctx.attack(enemy)
    {
        return BtResult::Success;
    }
    BtResult::Failure
}

// ── RPG helpers ─────────────────────────────────────────────────────────────

/// Stateful wander: picks a random point within 20 yards, commits the
/// destination to the blackboard, and keeps returning Running until the bot
/// arrives. Only rerolls after arrival.
///
/// Without this statefulness `get_random_point_nearby` would be called every
/// tick, rerouting the bot to a different point continuously and never
/// actually letting it arrive anywhere. The arrival threshold (3 yards²≈sqrt
/// distance 1.7y) matches the stop distance of the movement system.
fn tick_rpg_wander(ctx: &mut TickContext<'_>) -> BtResult {
    use crate::engine::blackboard::{Key, Value};

    const ARRIVAL_SQ: f32 = 3.0 * 3.0;

    // Resume an in-progress destination if one is saved.
    if let (Some(dx), Some(dy), Some(dz)) = (
        ctx.blackboard.get_f32(Key::RpgWanderDestX),
        ctx.blackboard.get_f32(Key::RpgWanderDestY),
        ctx.blackboard.get_f32(Key::RpgWanderDestZ),
    ) {
        let pos = &ctx.snap.self_.pos;
        let dist_sq = (pos.x - dx).powi(2) + (pos.y - dy).powi(2);
        if dist_sq <= ARRIVAL_SQ {
            // Arrived — clear the destination so the next activation rerolls.
            ctx.blackboard.clear(Key::RpgWanderDestX);
            ctx.blackboard.clear(Key::RpgWanderDestY);
            ctx.blackboard.clear(Key::RpgWanderDestZ);
            return BtResult::Success;
        }
        if ctx.interface.move_to(dx, dy, dz) {
            return BtResult::Running;
        }
        // Movement request failed — drop the destination so we can reroll
        // next time and fall through to Failure.
        ctx.blackboard.clear(Key::RpgWanderDestX);
        ctx.blackboard.clear(Key::RpgWanderDestY);
        ctx.blackboard.clear(Key::RpgWanderDestZ);
        return BtResult::Failure;
    }

    // No saved destination — pick a new one.
    if let Some(pos) = ctx.interface.get_random_point_nearby(20.0)
        && ctx.interface.move_to(pos.x, pos.y, pos.z)
    {
        ctx.blackboard.set(Key::RpgWanderDestX, Value::F32(pos.x));
        ctx.blackboard.set(Key::RpgWanderDestY, Value::F32(pos.y));
        ctx.blackboard.set(Key::RpgWanderDestZ, Value::F32(pos.z));
        return BtResult::Running;
    }
    BtResult::Failure
}

fn tick_rpg_interact(ctx: &mut TickContext<'_>) -> BtResult {
    let npcs = ctx.interface.get_nearby_gossip_npcs(15.0);
    if let Some(&npc) = npcs.first() {
        return approach_and_interact(ctx, npc, |_| BtResult::Success);
    }
    BtResult::Failure
}

/// Common emote IDs (TEXTEMOTE_*).
const EMOTES: &[u32] = &[
    1,   // WAVE
    2,   // BOW
    3,   // DANCE
    21,  // CHEER
    22,  // CHICKEN
    41,  // SALUTE
    78,  // HELLO
    101, // POINT
];

fn tick_rpg_emote(ctx: &mut TickContext<'_>) -> BtResult {
    // Pick a pseudo-random emote based on server time.
    let idx = (ctx.server_time_ms / 1000) as usize % EMOTES.len();
    if ctx.interface.emote(EMOTES[idx]) {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

// ── Gathering helpers ───────────────────────────────────────────────────────

fn tick_gather(ctx: &mut TickContext<'_>) -> BtResult {
    let nodes = ctx.interface.get_nearby_gatherables(30.0);
    if let Some(&node) = nodes.first() {
        let dist = ctx.interface.gameobject_distance(node);
        if dist > 5.0 {
            let pos = ctx.interface.gameobject_position(node);
            if ctx.interface.move_to(pos.x, pos.y, pos.z) {
                return BtResult::Running;
            }
        } else if ctx.interface.gather_node(node) {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounters::{EncounterEvent, EncounterFsm};
    use crate::engine::context::tests::{
        TestCtxOwned, TestInterface, make_encounter_ctx, make_test_ctx_with,
    };
    use crate::engine::blackboard::Blackboard;
    use crate::engine::throttles::Throttles;
    use crate::engine::timers::BotTimers;
    use crate::ffi::{BotRole, BotUnitSnapshot};

    struct MockEncounter {
        active: bool,
    }
    impl EncounterFsm for MockEncounter {
        fn update(&mut self, _: &EncounterEvent, _: f32, _: u64) {}
        fn phase_id(&self) -> u32 {
            1
        }
        fn is_active(&self) -> bool {
            self.active
        }
        fn is_done(&self) -> bool {
            false
        }
        fn boss_entry(&self) -> u32 {
            1
        }
    }

    #[test]
    fn seq_succeeds_when_all_succeed() {
        use Bt::*;
        let tree = Seq(vec![IsTank, IsTank]);
        let enc = MockEncounter { active: true };
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(
            &mut owned,
            &iface,
            &enc,
            PlayerClass::Warrior,
            BotRole::TANK,
        );
        assert_eq!(tree.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn seq_fails_on_first_failure() {
        use Bt::*;
        let tree = Seq(vec![IsTank, IsRanged]);
        let enc = MockEncounter { active: true };
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(
            &mut owned,
            &iface,
            &enc,
            PlayerClass::Warrior,
            BotRole::TANK,
        );
        assert_eq!(tree.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn sel_returns_first_success() {
        use Bt::*;
        let tree = Sel(vec![IsRanged, IsTank]);
        let enc = MockEncounter { active: true };
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(
            &mut owned,
            &iface,
            &enc,
            PlayerClass::Warrior,
            BotRole::TANK,
        );
        assert_eq!(tree.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn has_debuff_checks_self() {
        let spell = SpellId(12345);
        let tree = Bt::self_has(spell);
        let enc = MockEncounter { active: true };

        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &enc, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Failure);

        let iface = TestInterface::new().with_aura(spell);
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &enc, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn class_guard_filters() {
        use Bt::*;
        let tree = Seq(vec![IsClass(PlayerClass::Mage), HoldPosition]);
        let enc = MockEncounter { active: true };
        let iface = TestInterface::new();

        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &enc, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Success);

        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &enc, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn not_inverts() {
        use Bt::*;
        let tree = IsTank.not();
        let enc = MockEncounter { active: true };
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &enc, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn move_away_from_raid_with_safe_pos() {
        use Bt::*;
        let tree = MoveAwayFromRaid(40.0);
        let enc = MockEncounter { active: true };
        let iface = TestInterface::new().with_safe_pos();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &enc, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Running);
    }

    #[test]
    fn attack_nearest_succeeds_with_targets() {
        use Bt::*;
        let tree = AttackNearest;
        let enc = MockEncounter { active: true };
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        owned.attackers = vec![42];
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &enc, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn throttle_blocks_rapid_ticks() {
        let tree = Bt::throttle(5_000, Bt::AttackNearest);
        let mut owned = TestCtxOwned::new();
        owned.attackers = vec![42];
        owned.time_ms = 10_000;

        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success);

        // Second tick within throttle window → Failure.
        owned.time_ms = 12_000;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);

        // After throttle window → Success again.
        owned.time_ms = 16_000;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success);
    }

    #[test]
    fn consumables_recovery() {
        use Bt::*;
        // Low HP, out of combat → Running (recovering).
        let tree = Seq(vec![InCombat.not(), Consumables]);
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.health = 500;
        owned.snap.self_.max_health = 1000;
        owned.snap.self_.mana = 1000;
        owned.snap.self_.max_mana = 1000;
        owned.snap.self_.power_type = 0;
        owned.snap.self_.in_combat = false;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Running);

        // Full HP → Failure (nothing to do).
        owned.snap.self_.health = 1000;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn mode_condition() {
        use Bt::*;
        let tree = ModeIs(BehaviorMode::Stay);
        let mut owned = TestCtxOwned::new();
        owned.settings.mode = BehaviorMode::Stay;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success);

        owned.settings.mode = BehaviorMode::Follow;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn setting_enabled() {
        use Bt::*;
        let tree = SettingEnabled(Setting::AutoLoot);
        let mut owned = TestCtxOwned::new();
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success); // default is true

        owned.settings.auto_loot = false;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);
    }

    // ── RPG wander (stateful destination) ────────────────────────────────

    #[test]
    fn rpg_wander_commits_destination_on_first_tick() {
        use crate::engine::blackboard::Key;

        let iface = TestInterface::new().with_wander_point(100.0, 200.0, 50.0);
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.pos.x = 0.0;
        owned.snap.self_.pos.y = 0.0;
        owned.snap.self_.pos.z = 50.0;

        let mut bb = owned.blackboard;
        let mut timers = owned.timers;
        let mut throttles = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &iface,
            &mut bb,
            &mut timers,
            &mut throttles,
        );

        assert_eq!(Bt::RpgWander.tick(&mut ctx), BtResult::Running);
        assert_eq!(ctx.blackboard.get_f32(Key::RpgWanderDestX), Some(100.0));
        assert_eq!(ctx.blackboard.get_f32(Key::RpgWanderDestY), Some(200.0));
    }

    #[test]
    fn rpg_wander_keeps_destination_while_in_transit() {
        use crate::engine::blackboard::{Key, Value};

        // Pre-populate a saved destination far from current position.
        let iface = TestInterface::new().with_wander_point(999.0, 999.0, 50.0);
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.pos.x = 0.0;
        owned.snap.self_.pos.y = 0.0;
        owned.blackboard.set(Key::RpgWanderDestX, Value::F32(100.0));
        owned.blackboard.set(Key::RpgWanderDestY, Value::F32(200.0));
        owned.blackboard.set(Key::RpgWanderDestZ, Value::F32(50.0));

        let mut bb = owned.blackboard;
        let mut timers = owned.timers;
        let mut throttles = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &iface,
            &mut bb,
            &mut timers,
            &mut throttles,
        );

        assert_eq!(Bt::RpgWander.tick(&mut ctx), BtResult::Running);
        // Must not have been rerolled to the interface's wander_point.
        assert_eq!(ctx.blackboard.get_f32(Key::RpgWanderDestX), Some(100.0));
        assert_eq!(ctx.blackboard.get_f32(Key::RpgWanderDestY), Some(200.0));
    }

    #[test]
    fn rpg_wander_clears_destination_on_arrival() {
        use crate::engine::blackboard::{Key, Value};

        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        // Position is essentially at the saved destination.
        owned.snap.self_.pos.x = 100.5;
        owned.snap.self_.pos.y = 200.5;
        owned.blackboard.set(Key::RpgWanderDestX, Value::F32(100.0));
        owned.blackboard.set(Key::RpgWanderDestY, Value::F32(200.0));
        owned.blackboard.set(Key::RpgWanderDestZ, Value::F32(50.0));

        let mut bb = owned.blackboard;
        let mut timers = owned.timers;
        let mut throttles = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &iface,
            &mut bb,
            &mut timers,
            &mut throttles,
        );

        assert_eq!(Bt::RpgWander.tick(&mut ctx), BtResult::Success);
        assert!(matches!(
            ctx.blackboard.get(Key::RpgWanderDestX),
            crate::engine::blackboard::Value::None
        ));
    }

    // ── Throttle running-transparency ────────────────────────────────────

    #[test]
    fn throttle_bypasses_cooldown_while_child_running() {
        use crate::engine::blackboard::{Key, Value};

        // Wrap a stateful RpgWander in a throttle so we can observe that the
        // throttle ticks the child on consecutive ticks even though the
        // cooldown window has not yet elapsed.
        let tree = Bt::throttle(10_000, Bt::RpgWander);
        let iface = TestInterface::new().with_wander_point(100.0, 200.0, 50.0);
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.pos.x = 0.0;
        owned.snap.self_.pos.y = 0.0;

        let mut bb = owned.blackboard;
        let mut timers = owned.timers;
        let mut throttles = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &iface,
            &mut bb,
            &mut timers,
            &mut throttles,
        );

        // Tick 1: picks dest, move_to → Running, throttle marks fired+running.
        ctx.server_time_ms = 10_000;
        assert_eq!(tree.tick(&mut ctx), BtResult::Running);
        assert_eq!(ctx.blackboard.get_f32(Key::RpgWanderDestX), Some(100.0));

        // Tick 2: cooldown not elapsed, but child was Running last tick, so
        // throttle must bypass the check and keep ticking. Without the fix
        // this would return Failure and the Sel parent would fall through.
        ctx.server_time_ms = 10_500;
        assert_eq!(tree.tick(&mut ctx), BtResult::Running);

        // Place bot at destination. Next tick child returns Success and the
        // running flag clears.
        ctx.blackboard.set(Key::RpgWanderDestX, Value::F32(100.0));
        ctx.blackboard.set(Key::RpgWanderDestY, Value::F32(200.0));
        // Simulate arrival by tweaking the snap — we can't mutate snap
        // through the ctx borrow, so this case is covered by the dedicated
        // `rpg_wander_clears_destination_on_arrival` test above.
    }

    #[test]
    fn tick_follow_fails_without_any_target() {
        // Solo bot: no master, no group tank, empty group → Failure so the
        // selector in `mode_dispatch` falls through to RPG/Grind.
        let mut owned = TestCtxOwned::new();
        let result = tick_follow(&mut owned.ctx());
        assert_eq!(result, BtResult::Failure);
    }

    #[test]
    fn tick_follow_succeeds_with_master_in_range() {
        // Master set, default TestInterface returns unit_dist = 10.0 which is
        // > REFOLLOW_THRESHOLD (8.0), so the follow path kicks in and we get
        // Success back. This also exercises the formation dispatch path:
        // `Near` is the default formation and produces a `Position` output,
        // which goes through `interface.move_to` (stub returns true).
        let mut owned = TestCtxOwned::new();
        let mut ctx = owned.ctx();
        ctx.master_guid = Some(0xDEAD_BEEF);
        ctx.bot_handle = 0x1234_5678;
        assert_eq!(tick_follow(&mut ctx), BtResult::Success);
    }

    #[test]
    fn tick_follow_is_sticky_when_close_to_target() {
        // Master in chase range → don't re-issue, just return Success.
        // The existing TestInterface has a configurable `unit_dist`; set it
        // below the re-follow threshold and make sure we still report
        // Success (bot is already following) without needing to dispatch a
        // formation at all.
        use crate::engine::context::tests::TestInterface;
        let iface = TestInterface::new().with_unit_dist(2.0);
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &iface,
            &mut owned.blackboard,
            &mut owned.timers,
            &mut owned.throttles,
        );
        ctx.master_guid = Some(0xDEAD_BEEF);
        ctx.bot_handle = 0x1234_5678;
        assert_eq!(tick_follow(&mut ctx), BtResult::Success);
    }

    // ── 11a: target / location / group condition leaves ─────────────────

    /// Build a `TickContext` backed by the owned test state **and** a
    /// custom interface, preserving `owned.settings`. The standard
    /// `make_test_ctx_with` helper hardcodes a static default settings
    /// reference, which doesn't work for tests that need both a custom
    /// interface and custom settings (e.g. `preferred_rti_icon`,
    /// `protect_target`, `mode`). This helper bridges that gap.
    fn ctx_with_iface<'a>(
        owned: &'a mut TestCtxOwned,
        iface: &'a dyn crate::ffi::interface::BotInterface,
    ) -> TickContext<'a> {
        TickContext {
            snap: &owned.snap,
            nearby: &owned.nearby,
            attackers: &owned.attackers,
            group_state: None,
            interface: iface,
            blackboard: &mut owned.blackboard,
            timers: &mut owned.timers,
            throttles: &mut owned.throttles,
            server_time_ms: owned.time_ms,
            elapsed_ms: 100,
            minimal: false,
            bot_handle: 0,
            master_guid: None,
            encounter: None,
            class: PlayerClass::Warrior,
            role: BotRole::DPS,
            settings: &owned.settings,
            monitor_trace: None,
            pending_target: std::cell::Cell::new(None),
        }
    }

    /// Local mock that lets 11a condition tests inject unit snapshots,
    /// tank/healer handles, LOS flags, and unit distances. Only the
    /// methods the 11a handlers touch are overridden — everything else
    /// falls through to the surrounding `NullInterface`-style defaults.
    struct Mock11a {
        target_unit: BotUnitSnapshot,
        protect_unit: BotUnitSnapshot,
        tank: Option<UnitHandle>,
        healer: Option<UnitHandle>,
        has_los: bool,
        unit_distance: f32,
        /// Kind byte returned for any handle passed to `unit_kind`
        /// (0=other, 1=player, 2=pet, 3=critter). Mock11a doesn't
        /// bother with per-handle discrimination since each test only
        /// exercises a single target.
        target_kind: u8,
        /// 11c: dispel result — `Some((handle, spell))` → returned
        /// whenever the `dispel_mask` argument's bits overlap
        /// `dispel_mask_filter` (or `dispel_mask_filter == 0`, which
        /// means "any school"). Lets tests exercise the school filter
        /// gate without bringing up a full spell store.
        dispel_result: Option<(UnitHandle, SpellId)>,
        dispel_mask_filter: u8,
        /// 11c: dead party member handle (None = nobody to res).
        dead_member: Option<UnitHandle>,
        /// 11c: party-member snapshots keyed by handle — populated by
        /// `PartyMemberNeedsHeal` tests so `find_heal_target` sees a
        /// wounded member.
        group_snapshots: std::collections::HashMap<UnitHandle, BotUnitSnapshot>,
        /// 11c: item id returned from `find_potion_in_bags(0)` (buff
        /// category). `0` = no potion available.
        buff_potion_id: u32,
        /// 11c: shared potion cooldown state.
        potion_cd_ready: bool,
        /// 11d: PvP flag.
        pvp_flagged: bool,
        /// 11d: encoded duel state (0=none, 1=challenged, 2=in progress).
        duel_state: u8,
        /// 11d: faction → rank map. Missing entries fall back to 3
        /// (neutral), matching the FFI contract.
        rep_ranks: std::collections::HashMap<u32, u8>,
        /// 11e: item id → count map for `bot_item_count`. Missing
        /// entries resolve to `0` (PB2 contract for items not in bags).
        item_counts: std::collections::HashMap<u32, u32>,
        /// 11e: spells the bot "knows" for `knows_spell` / `HasRecipe`.
        /// Empty set means the bot knows nothing — overrides the trait
        /// default of `true` so the negative branch is testable.
        known_spells: std::collections::HashSet<u32>,
        /// 11e: quest log entries returned from `get_quest_log`. Empty
        /// = no quests (fresh character).
        quest_log: Vec<crate::ffi::interface::QuestInfo>,
        /// 11f: value returned from `auto_shoot(target)`. Defaults to
        /// `false` so the trait fallback stays engaged — tests that
        /// exercise the wand/bow branch of `PullTarget` flip it to
        /// `true`.
        auto_shoot_result: bool,
        /// 11g: raid-icon → unit map returned from
        /// `get_unit_with_raid_icon`. Keys use the FFI convention
        /// (1..=8, star..skull), not the 0..=7 settings convention —
        /// the BT handlers translate at the boundary. Missing keys
        /// resolve to `None`.
        raid_icon_units: std::collections::HashMap<u8, UnitHandle>,
        /// 11g: records of `(target, icon)` tuples passed to
        /// `group_set_target_icon`, appended in call order. Tests
        /// read this to assert the BT handler marked the right
        /// target with the right icon. `group_set_target_icon`
        /// returns `set_target_icon_result` regardless of whether
        /// the call was recorded.
        set_target_icon_log: std::cell::RefCell<Vec<(UnitHandle, u8)>>,
        /// 11g: value returned from `group_set_target_icon`. Default
        /// `true` — the happy path is "mark succeeded".
        set_target_icon_result: bool,
        /// 11h: utility potion item id returned from
        /// `find_potion_in_bags(1)`. `0` = no potion available.
        utility_potion_id: u32,
        /// 11h: value returned from `use_trinket(slot)`. Default `false`.
        use_trinket_result: bool,
        /// 11h: log of `use_trinket` calls (slot values, in order).
        use_trinket_log: std::cell::RefCell<Vec<u8>>,
        /// 11h: log of `use_item` calls (item_id values, in order).
        use_item_log: std::cell::RefCell<Vec<u32>>,
        /// 11h: value returned from `use_item`. Default `true`.
        use_item_result: bool,
        /// 11i: results for social/group actions. Each defaults to false.
        accept_group_invite_result: bool,
        leave_group_result: bool,
        accept_ready_check_result: bool,
        accept_trade_result: bool,
        accept_duel_result: bool,
        decline_duel_result: bool,
        accept_summon_result: bool,
        use_meeting_stone_result: bool,
    }

    impl Mock11a {
        fn new() -> Self {
            Self {
                target_unit: BotUnitSnapshot::default(),
                protect_unit: BotUnitSnapshot::default(),
                tank: None,
                healer: None,
                has_los: true,
                unit_distance: 10.0,
                target_kind: 0,
                dispel_result: None,
                dispel_mask_filter: 0,
                dead_member: None,
                group_snapshots: std::collections::HashMap::new(),
                buff_potion_id: 0,
                potion_cd_ready: true,
                pvp_flagged: false,
                duel_state: 0,
                rep_ranks: std::collections::HashMap::new(),
                item_counts: std::collections::HashMap::new(),
                known_spells: std::collections::HashSet::new(),
                quest_log: Vec::new(),
                auto_shoot_result: false,
                raid_icon_units: std::collections::HashMap::new(),
                set_target_icon_log: std::cell::RefCell::new(Vec::new()),
                set_target_icon_result: true,
                utility_potion_id: 0,
                use_trinket_result: false,
                use_trinket_log: std::cell::RefCell::new(Vec::new()),
                use_item_log: std::cell::RefCell::new(Vec::new()),
                use_item_result: true,
                accept_group_invite_result: false,
                leave_group_result: false,
                accept_ready_check_result: false,
                accept_trade_result: false,
                accept_duel_result: false,
                decline_duel_result: false,
                accept_summon_result: false,
                use_meeting_stone_result: false,
            }
        }
    }

    impl crate::ffi::interface::BotInterface for Mock11a {
        fn get_snapshot(&self) -> crate::ffi::BotWorldSnapshot {
            crate::ffi::BotWorldSnapshot::default()
        }
        fn get_unit_snapshot(&self, target: UnitHandle) -> BotUnitSnapshot {
            // Handle `1` = "current target"; handle `2` = "protect target".
            // Everything else falls through to the 11c `group_snapshots`
            // map so heal/party-member tests can stage wounded members
            // by handle.
            match target {
                1 => self.target_unit,
                2 => self.protect_unit,
                h => self
                    .group_snapshots
                    .get(&h)
                    .copied()
                    .unwrap_or_default(),
            }
        }
        fn has_aura(&self, _: UnitHandle, _: SpellId) -> bool {
            false
        }
        fn get_aura(&self, _: UnitHandle, _: SpellId) -> Option<crate::ffi::BotAuraInfo> {
            None
        }
        fn get_auras(&self, _: UnitHandle) -> Vec<crate::ffi::BotAuraInfo> {
            vec![]
        }
        fn get_threat_list(&self, _: UnitHandle) -> Vec<crate::ffi::BotThreatEntry> {
            vec![]
        }
        fn get_unit_threat(&self, _: UnitHandle, _: UnitHandle) -> f32 {
            0.0
        }
        fn unit_distance(&self, _: UnitHandle) -> f32 {
            self.unit_distance
        }
        fn can_cast(&self, _: SpellId, _: UnitHandle) -> bool {
            true
        }
        fn spell_cooldown_ms(&self, _: SpellId) -> u32 {
            0
        }
        fn has_los(&self, _: UnitHandle) -> bool {
            self.has_los
        }
        fn get_nearby_units(&self, _: f32, _: bool) -> Vec<UnitHandle> {
            vec![]
        }
        fn get_behind_position(&self, _: UnitHandle, _: f32) -> crate::ffi::BotPosition {
            Default::default()
        }
        fn get_safe_position(&self, _: f32) -> Option<crate::ffi::BotPosition> {
            None
        }
        fn get_spread_position(
            &self,
            _: UnitHandle,
            _: f32,
            _: u8,
            _: u8,
        ) -> crate::ffi::BotPosition {
            Default::default()
        }
        fn can_reach(&self, _: f32, _: f32, _: f32) -> bool {
            true
        }
        fn cast_spell(&self, _: SpellId, _: UnitHandle) -> bool {
            true
        }
        fn cast_spell_pos(&self, _: SpellId, _: f32, _: f32, _: f32) -> bool {
            true
        }
        fn move_to(&self, _: f32, _: f32, _: f32) -> bool {
            true
        }
        fn follow(&self, _: UnitHandle, _: f32, _: f32) -> bool {
            true
        }
        fn stop_moving(&self) -> bool {
            true
        }
        fn attack(&self, _: UnitHandle) -> bool {
            true
        }
        fn auto_attack(&self, _: bool) -> bool {
            true
        }
        fn say(&self, _: &str, _: u32) -> bool {
            true
        }
        fn use_item(&self, item_id: crate::ffi::ItemId, _target: UnitHandle) -> bool {
            self.use_item_log.borrow_mut().push(item_id.raw());
            self.use_item_result
        }
        fn taunt(&self, _: UnitHandle) -> bool {
            true
        }
        fn group_get_tank(&self) -> Option<UnitHandle> {
            self.tank
        }
        fn group_get_healer(&self) -> Option<UnitHandle> {
            self.healer
        }
        fn group_get_role(&self, _: UnitHandle) -> crate::ffi::BotRole {
            Default::default()
        }
        fn unit_kind(&self, _: UnitHandle) -> u8 {
            self.target_kind
        }
        fn find_dispellable_target(&self, dispel_mask: u8) -> Option<(UnitHandle, SpellId)> {
            // Parity with the C++ side: `0` = any school. Otherwise only
            // return a hit when the caller's mask overlaps the mock's
            // configured filter.
            if self.dispel_result.is_none() {
                return None;
            }
            if dispel_mask == 0
                || self.dispel_mask_filter == 0
                || (dispel_mask & self.dispel_mask_filter) != 0
            {
                self.dispel_result
            } else {
                None
            }
        }
        fn find_dead_party_member(&self) -> Option<UnitHandle> {
            self.dead_member
        }
        fn find_potion_in_bags(&self, category: u8) -> ItemId {
            match category {
                0 => ItemId(self.buff_potion_id),
                1 => ItemId(self.utility_potion_id),
                _ => ItemId(0),
            }
        }
        fn potion_cooldown_ready(&self) -> bool {
            self.potion_cd_ready
        }
        fn is_pvp_flagged(&self) -> bool {
            self.pvp_flagged
        }
        fn duel_state(&self) -> u8 {
            self.duel_state
        }
        fn reputation_rank(&self, faction_id: u32) -> u8 {
            self.rep_ranks.get(&faction_id).copied().unwrap_or(3)
        }
        fn bot_item_count(&self, item_id: ItemId) -> u32 {
            self.item_counts.get(&item_id.raw()).copied().unwrap_or(0)
        }
        fn knows_spell(&self, spell_id: SpellId) -> bool {
            self.known_spells.contains(&spell_id.raw())
        }
        fn get_quest_log(&self) -> Vec<crate::ffi::interface::QuestInfo> {
            self.quest_log.clone()
        }
        fn auto_shoot(&self, _target: UnitHandle) -> bool {
            self.auto_shoot_result
        }
        fn get_unit_with_raid_icon(&self, icon: u8) -> Option<UnitHandle> {
            self.raid_icon_units.get(&icon).copied()
        }
        fn group_set_target_icon(&self, target: UnitHandle, icon: u8) -> bool {
            self.set_target_icon_log.borrow_mut().push((target, icon));
            self.set_target_icon_result
        }
        fn use_trinket(&self, slot: u8) -> bool {
            self.use_trinket_log.borrow_mut().push(slot);
            self.use_trinket_result
        }
        fn accept_group_invite(&self) -> bool {
            self.accept_group_invite_result
        }
        fn leave_group(&self) -> bool {
            self.leave_group_result
        }
        fn accept_ready_check(&self) -> bool {
            self.accept_ready_check_result
        }
        fn accept_trade(&self) -> bool {
            self.accept_trade_result
        }
        fn accept_duel(&self) -> bool {
            self.accept_duel_result
        }
        fn decline_duel(&self) -> bool {
            self.decline_duel_result
        }
        fn accept_summon(&self) -> bool {
            self.accept_summon_result
        }
        fn use_meeting_stone(&self) -> bool {
            self.use_meeting_stone_result
        }
    }

    #[test]
    fn target_casting_spell_matches_snapshot() {
        let spell = SpellId(12345);
        let mut mock = Mock11a::new();
        mock.target_unit.is_casting = true;
        mock.target_unit.casting_spell_id = spell.raw();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut bb = owned.blackboard;
        let mut tm = owned.timers;
        let mut th = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &mock,
            &mut bb,
            &mut tm,
            &mut th,
        );
        assert_eq!(Bt::TargetCastingSpell(spell).tick(&mut ctx), BtResult::Success);
        assert_eq!(
            Bt::TargetCastingSpell(SpellId(999)).tick(&mut ctx),
            BtResult::Failure
        );
    }

    #[test]
    fn target_casting_spell_fails_without_target() {
        let mut owned = TestCtxOwned::new();
        // current_target stays 0.
        assert_eq!(
            Bt::TargetCastingSpell(SpellId(1)).tick(&mut owned.ctx()),
            BtResult::Failure
        );
    }

    #[test]
    fn random_chance_bounds() {
        let mut owned = TestCtxOwned::new();
        assert_eq!(Bt::RandomChance(0).tick(&mut owned.ctx()), BtResult::Failure);
        assert_eq!(Bt::RandomChance(100).tick(&mut owned.ctx()), BtResult::Success);
        assert_eq!(Bt::RandomChance(200).tick(&mut owned.ctx()), BtResult::Success);
    }

    #[test]
    fn random_chance_is_deterministic_per_tick() {
        // Same (time, bot_handle, pct) must always yield the same result.
        let mut owned = TestCtxOwned::new();
        owned.time_ms = 42_000;
        let first = Bt::RandomChance(50).tick(&mut owned.ctx());
        let second = Bt::RandomChance(50).tick(&mut owned.ctx());
        assert_eq!(first, second);
    }

    #[test]
    fn in_zone_and_in_map() {
        let mut owned = TestCtxOwned::new();
        owned.snap.zone_id = 1519;
        owned.snap.self_.pos.map_id = 0;
        assert_eq!(Bt::InZone(1519).tick(&mut owned.ctx()), BtResult::Success);
        assert_eq!(Bt::InZone(9999).tick(&mut owned.ctx()), BtResult::Failure);
        assert_eq!(Bt::InMap(0).tick(&mut owned.ctx()), BtResult::Success);
        assert_eq!(Bt::InMap(530).tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn rti_assist_target_valid_uses_preferred_icon() {
        let mut owned = TestCtxOwned::new();
        owned.snap.group_size = 2;
        owned.snap.group_raid_target_icons[7] = 0xABCD; // skull
        // Default preference is None → falls through to skull (7).
        assert_eq!(
            Bt::RtiAssistTargetValid.tick(&mut owned.ctx()),
            BtResult::Success
        );
        // Point preference at an empty slot — now fails.
        owned.settings.preferred_rti_icon = Some(4);
        assert_eq!(
            Bt::RtiAssistTargetValid.tick(&mut owned.ctx()),
            BtResult::Failure
        );
    }

    #[test]
    fn rti_cc_target_valid_uses_preferred_icon() {
        let mut owned = TestCtxOwned::new();
        owned.snap.group_size = 2;
        owned.snap.group_raid_target_icons[5] = 0xCAFE; // square
        assert_eq!(
            Bt::RtiCcTargetValid.tick(&mut owned.ctx()),
            BtResult::Success
        );
        owned.snap.group_raid_target_icons[5] = 0;
        assert_eq!(
            Bt::RtiCcTargetValid.tick(&mut owned.ctx()),
            BtResult::Failure
        );
    }

    #[test]
    fn party_no_tank_healer_requires_group() {
        // Solo bot → both fail (PB2 doesn't complain about missing tank
        // when you're alone).
        let mut owned = TestCtxOwned::new();
        assert_eq!(Bt::PartyNoTank.tick(&mut owned.ctx()), BtResult::Failure);
        assert_eq!(Bt::PartyNoHealer.tick(&mut owned.ctx()), BtResult::Failure);

        // In a group, NullInterface returns None for both → Success.
        owned.snap.group_size = 3;
        assert_eq!(Bt::PartyNoTank.tick(&mut owned.ctx()), BtResult::Success);
        assert_eq!(Bt::PartyNoHealer.tick(&mut owned.ctx()), BtResult::Success);
    }

    #[test]
    fn party_no_tank_is_false_when_tank_exists() {
        let mut mock = Mock11a::new();
        mock.tank = Some(0xDEAD);
        let mut owned = TestCtxOwned::new();
        owned.snap.group_size = 3;
        let mut bb = owned.blackboard;
        let mut tm = owned.timers;
        let mut th = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &mock,
            &mut bb,
            &mut tm,
            &mut th,
        );
        assert_eq!(Bt::PartyNoTank.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn in_los_of_master_requires_master_and_los() {
        // No master → Failure.
        let mut owned = TestCtxOwned::new();
        assert_eq!(Bt::InLosOfMaster.tick(&mut owned.ctx()), BtResult::Failure);

        // Master set + NullInterface returns has_los=true → Success.
        let mut ctx = owned.ctx();
        ctx.master_guid = Some(0xBEEF);
        assert_eq!(Bt::InLosOfMaster.tick(&mut ctx), BtResult::Success);

        // With Mock11a we can force has_los=false.
        let mock = Mock11a { has_los: false, ..Mock11a::new() };
        let mut bb = owned.blackboard;
        let mut tm = owned.timers;
        let mut th = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &mock,
            &mut bb,
            &mut tm,
            &mut th,
        );
        ctx.master_guid = Some(0xBEEF);
        assert_eq!(Bt::InLosOfMaster.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn in_react_range_of_master_uses_distance() {
        let mock = Mock11a { unit_distance: 100.0, ..Mock11a::new() };
        let mut owned = TestCtxOwned::new();
        let mut bb = owned.blackboard;
        let mut tm = owned.timers;
        let mut th = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &mock,
            &mut bb,
            &mut tm,
            &mut th,
        );
        ctx.master_guid = Some(0xBEEF);
        assert_eq!(Bt::InReactRangeOfMaster.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn in_react_range_of_master_fails_when_out_of_range() {
        let mock = Mock11a { unit_distance: 300.0, ..Mock11a::new() };
        let mut owned = TestCtxOwned::new();
        let mut bb = owned.blackboard;
        let mut tm = owned.timers;
        let mut th = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &mock,
            &mut bb,
            &mut tm,
            &mut th,
        );
        ctx.master_guid = Some(0xBEEF);
        assert_eq!(Bt::InReactRangeOfMaster.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn is_following_master_mode_gated() {
        let mut owned = TestCtxOwned::new();
        // Default mode is Follow; no master set → Failure.
        owned.settings.mode = BehaviorMode::Follow;
        assert_eq!(
            Bt::IsFollowingMaster.tick(&mut owned.ctx()),
            BtResult::Failure
        );
        // Master set → Success.
        {
            let mut ctx = owned.ctx();
            ctx.master_guid = Some(0xFEED);
            assert_eq!(Bt::IsFollowingMaster.tick(&mut ctx), BtResult::Success);
        }
        // Mode not Follow → Failure even with master.
        owned.settings.mode = BehaviorMode::Stay;
        let mut ctx = owned.ctx();
        ctx.master_guid = Some(0xFEED);
        assert_eq!(Bt::IsFollowingMaster.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn is_following_tank_requires_group_tank() {
        let mut mock = Mock11a::new();
        mock.tank = Some(0x1234);
        let mut owned = TestCtxOwned::new();
        owned.settings.mode = BehaviorMode::Follow;
        let mut bb = owned.blackboard;
        let mut tm = owned.timers;
        let mut th = owned.throttles;
        let mut ctx = make_test_ctx_with(
            &owned.snap,
            &owned.nearby,
            &owned.attackers,
            &mock,
            &mut bb,
            &mut tm,
            &mut th,
        );
        ctx.bot_handle = 0x9999;
        assert_eq!(Bt::IsFollowingTank.tick(&mut ctx), BtResult::Success);
        // Tank == self → Failure.
        ctx.bot_handle = 0x1234;
        assert_eq!(Bt::IsFollowingTank.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn has_protect_target_damaged() {
        let mut mock = Mock11a::new();
        mock.protect_unit.is_alive = true;
        mock.protect_unit.health = 800;
        mock.protect_unit.max_health = 1000;
        let mut owned = TestCtxOwned::new();
        owned.settings.protect_target = Some(2);
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::HasProtectTargetDamaged.tick(&mut ctx),
            BtResult::Success
        );
    }

    #[test]
    fn has_protect_target_damaged_at_full_hp_fails() {
        let mut mock = Mock11a::new();
        mock.protect_unit.is_alive = true;
        mock.protect_unit.health = 1000;
        mock.protect_unit.max_health = 1000;
        let mut owned = TestCtxOwned::new();
        owned.settings.protect_target = Some(2);
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::HasProtectTargetDamaged.tick(&mut ctx),
            BtResult::Failure
        );
    }

    #[test]
    fn has_protect_target_damaged_fails_without_target() {
        let mut owned = TestCtxOwned::new();
        assert_eq!(
            Bt::HasProtectTargetDamaged.tick(&mut owned.ctx()),
            BtResult::Failure
        );
    }

    // ── 11b: target-type conditions ─────────────────────────────────────

    #[test]
    fn target_is_player_matches_unit_kind_1() {
        let mock = Mock11a { target_kind: 1, ..Mock11a::new() };
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::TargetIsPlayer.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::TargetIsPet.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::TargetIsCritter.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn target_is_pet_matches_unit_kind_2() {
        let mock = Mock11a { target_kind: 2, ..Mock11a::new() };
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::TargetIsPet.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::TargetIsPlayer.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::TargetIsCritter.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn target_is_critter_matches_unit_kind_3() {
        let mock = Mock11a { target_kind: 3, ..Mock11a::new() };
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::TargetIsCritter.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::TargetIsPlayer.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::TargetIsPet.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn target_type_conditions_fail_without_target() {
        // current_target = 0 → all three target-type gates short-circuit
        // to Failure regardless of what `unit_kind` would report.
        let mock = Mock11a { target_kind: 1, ..Mock11a::new() };
        let mut owned = TestCtxOwned::new();
        // Default snapshot has current_target == 0.
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::TargetIsPlayer.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::TargetIsPet.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::TargetIsCritter.tick(&mut ctx), BtResult::Failure);
    }

    // ── 11c: party / dispel / res / consumables ─────────────────────────

    #[test]
    fn party_member_needs_dispel_any_school() {
        // Mock has a hit, no filter → `Any` returns Success.
        let mock = Mock11a {
            dispel_result: Some((0x42, SpellId(100))),
            ..Mock11a::new()
        };
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::PartyMemberNeedsDispel(DispelSchool::Any).tick(&mut ctx),
            BtResult::Success
        );
    }

    #[test]
    fn party_member_needs_dispel_empty_returns_failure() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::PartyMemberNeedsDispel(DispelSchool::Any).tick(&mut ctx),
            BtResult::Failure
        );
        assert_eq!(
            Bt::PartyMemberNeedsDispel(DispelSchool::Magic).tick(&mut ctx),
            BtResult::Failure
        );
    }

    #[test]
    fn party_member_needs_dispel_school_filter_matches() {
        // Mock carries a "curse" debuff; Magic request misses, Curse hits.
        let mock = Mock11a {
            dispel_result: Some((0x42, SpellId(100))),
            dispel_mask_filter: DispelSchool::Curse.mask(),
            ..Mock11a::new()
        };
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::PartyMemberNeedsDispel(DispelSchool::Curse).tick(&mut ctx),
            BtResult::Success
        );
        assert_eq!(
            Bt::PartyMemberNeedsDispel(DispelSchool::Magic).tick(&mut ctx),
            BtResult::Failure
        );
        // `Any` (mask 0) must still match — that's the parity contract
        // with the C++ side.
        assert_eq!(
            Bt::PartyMemberNeedsDispel(DispelSchool::Any).tick(&mut ctx),
            BtResult::Success
        );
    }

    #[test]
    fn dispel_school_mask_bits_match_server_layout() {
        // Magic=1, Curse=2, Disease=3, Poison=4 on the server → bits
        // 2/4/8/16. `Any` collapses to 0.
        assert_eq!(DispelSchool::Any.mask(), 0);
        assert_eq!(DispelSchool::Magic.mask(), 1 << 1);
        assert_eq!(DispelSchool::Curse.mask(), 1 << 2);
        assert_eq!(DispelSchool::Disease.mask(), 1 << 3);
        assert_eq!(DispelSchool::Poison.mask(), 1 << 4);
    }

    #[test]
    fn party_member_needs_res_uses_dead_party_lookup() {
        let mut mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        // No dead member → Failure.
        {
            let mut ctx = ctx_with_iface(&mut owned, &mock);
            assert_eq!(Bt::PartyMemberNeedsRes.tick(&mut ctx), BtResult::Failure);
        }
        // Dead member handle set → Success.
        mock.dead_member = Some(0x7777);
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::PartyMemberNeedsRes.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn party_member_needs_heal_uses_find_heal_target() {
        let mut mock = Mock11a::new();
        // Party member handle 0x10 sitting at 30% HP.
        let mut wounded = BotUnitSnapshot::default();
        wounded.is_alive = true;
        wounded.health = 30;
        wounded.max_health = 100;
        mock.group_snapshots.insert(0x10, wounded);

        let mut owned = TestCtxOwned::new();
        owned.snap.group_size = 2;
        owned.snap.group_members[0] = 0x10;
        owned.snap.self_.health = 100;
        owned.snap.self_.max_health = 100;

        let mut ctx = ctx_with_iface(&mut owned, &mock);
        ctx.bot_handle = 0x1;
        // 50% threshold catches the wounded member.
        assert_eq!(
            Bt::PartyMemberNeedsHeal(0.5).tick(&mut ctx),
            BtResult::Success
        );
        // 20% threshold does not.
        assert_eq!(
            Bt::PartyMemberNeedsHeal(0.2).tick(&mut ctx),
            BtResult::Failure
        );
    }

    #[test]
    fn has_buff_potion_available_reads_find_potion() {
        let mut mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        {
            let mut ctx = ctx_with_iface(&mut owned, &mock);
            assert_eq!(
                Bt::HasBuffPotionAvailable.tick(&mut ctx),
                BtResult::Failure
            );
        }
        mock.buff_potion_id = 13452; // Elixir of the Mongoose
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::HasBuffPotionAvailable.tick(&mut ctx),
            BtResult::Success
        );
    }

    #[test]
    fn potion_cooldown_ready_reads_flag() {
        let mut mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        // Default = true.
        {
            let mut ctx = ctx_with_iface(&mut owned, &mock);
            assert_eq!(Bt::PotionCooldownReady.tick(&mut ctx), BtResult::Success);
        }
        mock.potion_cd_ready = false;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::PotionCooldownReady.tick(&mut ctx), BtResult::Failure);
    }

    // ── 11d: PvP / duel / reputation ────────────────────────────────────

    #[test]
    fn pvp_flagged_reads_interface() {
        let mut mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        {
            let mut ctx = ctx_with_iface(&mut owned, &mock);
            assert_eq!(Bt::PvpFlagged.tick(&mut ctx), BtResult::Failure);
        }
        mock.pvp_flagged = true;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::PvpFlagged.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn duel_state_splits_requested_and_in_progress() {
        let mut mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        // State 0 — neither gate fires.
        {
            let mut ctx = ctx_with_iface(&mut owned, &mock);
            assert_eq!(Bt::DuelRequested.tick(&mut ctx), BtResult::Failure);
            assert_eq!(Bt::InDuel.tick(&mut ctx), BtResult::Failure);
        }
        // State 1 — only `DuelRequested` fires.
        mock.duel_state = 1;
        {
            let mut ctx = ctx_with_iface(&mut owned, &mock);
            assert_eq!(Bt::DuelRequested.tick(&mut ctx), BtResult::Success);
            assert_eq!(Bt::InDuel.tick(&mut ctx), BtResult::Failure);
        }
        // State 2 — only `InDuel` fires.
        mock.duel_state = 2;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::DuelRequested.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::InDuel.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn rep_with_faction_below_uses_strict_less_than() {
        let mut mock = Mock11a::new();
        // Faction 76 (Orgrimmar) at Friendly (4).
        mock.rep_ranks.insert(76, ReputationRank::Friendly.raw());
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        // Below Honored (5) → Friendly is strictly less → Success.
        assert_eq!(
            Bt::RepWithFactionBelow(76, ReputationRank::Honored).tick(&mut ctx),
            BtResult::Success
        );
        // Below Friendly (4) → Friendly is not strictly less → Failure.
        assert_eq!(
            Bt::RepWithFactionBelow(76, ReputationRank::Friendly).tick(&mut ctx),
            BtResult::Failure
        );
        // Below Exalted (7) → still Success.
        assert_eq!(
            Bt::RepWithFactionBelow(76, ReputationRank::Exalted).tick(&mut ctx),
            BtResult::Success
        );
    }

    #[test]
    fn rep_with_faction_below_defaults_unknown_faction_to_neutral() {
        // No entry in rep_ranks → mock returns 3 (neutral). A query
        // `< Friendly(4)` succeeds; a query `< Neutral(3)` fails.
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::RepWithFactionBelow(999, ReputationRank::Friendly).tick(&mut ctx),
            BtResult::Success
        );
        assert_eq!(
            Bt::RepWithFactionBelow(999, ReputationRank::Neutral).tick(&mut ctx),
            BtResult::Failure
        );
    }

    #[test]
    fn reputation_rank_enum_matches_server_layout() {
        assert_eq!(ReputationRank::Hated.raw(), 0);
        assert_eq!(ReputationRank::Hostile.raw(), 1);
        assert_eq!(ReputationRank::Unfriendly.raw(), 2);
        assert_eq!(ReputationRank::Neutral.raw(), 3);
        assert_eq!(ReputationRank::Friendly.raw(), 4);
        assert_eq!(ReputationRank::Honored.raw(), 5);
        assert_eq!(ReputationRank::Revered.raw(), 6);
        assert_eq!(ReputationRank::Exalted.raw(), 7);
    }

    // ── 11e: quest / recipe / item ──────────────────────────────────────

    #[test]
    fn item_in_bags_count_honours_threshold() {
        let mut mock = Mock11a::new();
        mock.item_counts.insert(6265, 3); // soul shards
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(
            Bt::ItemInBagsCount(ItemId(6265), 0).tick(&mut ctx),
            BtResult::Success
        );
        assert_eq!(
            Bt::ItemInBagsCount(ItemId(6265), 3).tick(&mut ctx),
            BtResult::Success
        );
        assert_eq!(
            Bt::ItemInBagsCount(ItemId(6265), 4).tick(&mut ctx),
            BtResult::Failure
        );
        // Unknown item id → count 0.
        assert_eq!(
            Bt::ItemInBagsCount(ItemId(9999), 1).tick(&mut ctx),
            BtResult::Failure
        );
    }

    #[test]
    fn has_recipe_forwards_to_knows_spell() {
        let mut mock = Mock11a::new();
        mock.known_spells.insert(2366); // Herbalism
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(
            Bt::HasRecipe(SpellId(2366)).tick(&mut ctx),
            BtResult::Success
        );
        assert_eq!(
            Bt::HasRecipe(SpellId(2575)).tick(&mut ctx),
            BtResult::Failure
        );
    }

    #[test]
    fn quest_in_log_active_matches_any_entry() {
        use crate::ffi::interface::QuestInfo;
        let mut mock = Mock11a::new();
        mock.quest_log.push(QuestInfo { quest_id: 42, complete: false });
        mock.quest_log.push(QuestInfo { quest_id: 77, complete: true });
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(Bt::QuestInLogActive(42).tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::QuestInLogActive(77).tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::QuestInLogActive(99).tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn quest_in_log_complete_requires_complete_flag() {
        use crate::ffi::interface::QuestInfo;
        let mut mock = Mock11a::new();
        mock.quest_log.push(QuestInfo { quest_id: 42, complete: false });
        mock.quest_log.push(QuestInfo { quest_id: 77, complete: true });
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        // 42 is in the log but not complete.
        assert_eq!(Bt::QuestInLogComplete(42).tick(&mut ctx), BtResult::Failure);
        // 77 is complete.
        assert_eq!(Bt::QuestInLogComplete(77).tick(&mut ctx), BtResult::Success);
        // 99 isn't in the log at all.
        assert_eq!(Bt::QuestInLogComplete(99).tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn quest_in_log_empty_log_is_failure() {
        let mock = Mock11a::new(); // empty quest_log
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::QuestInLogActive(1).tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::QuestInLogComplete(1).tick(&mut ctx), BtResult::Failure);
    }

    // ── 11f: generic movement / positioning ─────────────────────────────

    #[test]
    fn kite_from_target_fails_without_target() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new(); // current_target stays 0
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::KiteFromTarget(5.0).tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn kite_from_target_fails_when_already_out_of_range() {
        let mock = Mock11a {
            unit_distance: 20.0,
            ..Mock11a::new()
        };
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::KiteFromTarget(10.0).tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn kite_from_target_runs_when_in_range() {
        // Target at (0,0,0); bot at (3,4,0) → 5 yards away. Kite
        // distance 10 → bot is inside, must move.
        let mut mock = Mock11a::new();
        mock.unit_distance = 5.0;
        mock.target_unit.pos.x = 0.0;
        mock.target_unit.pos.y = 0.0;
        mock.target_unit.pos.z = 0.0;
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        owned.snap.self_.pos.x = 3.0;
        owned.snap.self_.pos.y = 4.0;
        owned.snap.self_.pos.z = 0.0;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::KiteFromTarget(10.0).tick(&mut ctx), BtResult::Running);
    }

    #[test]
    fn kite_from_target_fails_when_on_top_of_target() {
        let mut mock = Mock11a::new();
        mock.unit_distance = 0.0;
        // target & bot positions default to 0,0,0 — no kite direction.
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::KiteFromTarget(10.0).tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn close_to_target_runs_when_out_of_range() {
        let mock = Mock11a {
            unit_distance: 20.0,
            ..Mock11a::new()
        };
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::CloseToTarget(5.0).tick(&mut ctx), BtResult::Running);
    }

    #[test]
    fn close_to_target_fails_when_in_range() {
        let mock = Mock11a {
            unit_distance: 3.0,
            ..Mock11a::new()
        };
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::CloseToTarget(5.0).tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn close_to_target_fails_without_target() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new(); // current_target stays 0
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::CloseToTarget(5.0).tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn pull_target_fails_without_target() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::PullTarget.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn pull_target_uses_auto_shoot_when_available() {
        let mut mock = Mock11a::new();
        mock.auto_shoot_result = true;
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::PullTarget.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn pull_target_falls_back_to_taunt() {
        // auto_shoot_result is false by default; Mock11a::taunt always
        // returns true. Dispatch must land on the taunt path and
        // return Success without ever needing the attack fallback.
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::PullTarget.tick(&mut ctx), BtResult::Success);
    }

    // ── 11g: RTI / CC targeting ─────────────────────────────────────────

    #[test]
    fn mark_rti_wraps_group_set_target_icon() {
        let mock = Mock11a::new(); // set_target_icon_result = true
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(Bt::MarkRti(7).tick(&mut ctx), BtResult::Success);
        let log = mock.set_target_icon_log.borrow();
        assert_eq!(log.as_slice(), &[(1_u64, 7_u8)]);
    }

    #[test]
    fn mark_rti_cc_shares_wire_behaviour() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(Bt::MarkRtiCc(5).tick(&mut ctx), BtResult::Success);
        let log = mock.set_target_icon_log.borrow();
        assert_eq!(log.as_slice(), &[(1_u64, 5_u8)]);
    }

    #[test]
    fn mark_rti_without_target_is_failure() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new(); // current_target = 0
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(Bt::MarkRti(7).tick(&mut ctx), BtResult::Failure);
        assert!(mock.set_target_icon_log.borrow().is_empty());
    }

    #[test]
    fn mark_rti_rejects_out_of_range_icon() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(Bt::MarkRti(8).tick(&mut ctx), BtResult::Failure);
        assert!(mock.set_target_icon_log.borrow().is_empty());
    }

    #[test]
    fn mark_rti_propagates_interface_failure() {
        let mock = Mock11a {
            set_target_icon_result: false,
            ..Mock11a::new()
        };
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(Bt::MarkRti(7).tick(&mut ctx), BtResult::Failure);
        // The handler still invoked the FFI — we're confirming the
        // Failure propagation path, not a short-circuit.
        assert_eq!(mock.set_target_icon_log.borrow().len(), 1);
    }

    #[test]
    fn rti_assist_switches_to_icon_unit_and_attacks() {
        let mut mock = Mock11a::new();
        // Preferred rti icon defaults to 7 (skull, 0..7 indexing). The
        // BT translates to 8 (1..8 indexing) at the FFI boundary.
        mock.raid_icon_units.insert(8, 4242);
        let mut owned = TestCtxOwned::new();
        // Leave preferred_rti_icon at its default (None → 7 in handler).
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::RtiAssist.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn rti_assist_fails_when_no_unit_wears_icon() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::RtiAssist.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn rti_cc_target_select_uses_cc_icon() {
        let mut mock = Mock11a::new();
        // Default preferred_cc_rti_icon = None → handler uses 5
        // (square, 0..7) → translates to 6 (1..8).
        mock.raid_icon_units.insert(6, 9999);
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::RtiCcTargetSelect.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn cc_cast_on_rti_casts_on_icon_unit() {
        let mut mock = Mock11a::new();
        mock.raid_icon_units.insert(6, 7777); // square via 0..7→1..8
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::CcCastOnRti(SpellId(118)).tick(&mut ctx),
            BtResult::Success
        );
    }

    #[test]
    fn cc_cast_on_rti_fails_without_marked_unit() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::CcCastOnRti(SpellId(118)).tick(&mut ctx),
            BtResult::Failure
        );
    }

    #[test]
    fn cc_cast_on_nearest_picks_non_current_target() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        owned.nearby = vec![1, 2, 3]; // skip 1 (current), pick 2.
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::CcCastOnNearest(SpellId(118)).tick(&mut ctx),
            BtResult::Success
        );
    }

    #[test]
    fn cc_cast_on_nearest_fails_when_only_current_target_nearby() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 1;
        owned.nearby = vec![1];
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(
            Bt::CcCastOnNearest(SpellId(118)).tick(&mut ctx),
            BtResult::Failure
        );
    }

    // ── 11h: consumables / racials / trinkets ─────────────────────────

    #[test]
    fn use_buff_potion_consumes_item() {
        let mut mock = Mock11a::new();
        mock.buff_potion_id = 13452; // Elixir of the Mongoose
        mock.potion_cd_ready = true;
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::UseBuffPotion.tick(&mut ctx), BtResult::Success);
        let log = mock.use_item_log.borrow();
        assert_eq!(log.as_slice(), &[13452]);
    }

    #[test]
    fn use_buff_potion_fails_when_no_potion() {
        let mock = Mock11a::new(); // buff_potion_id = 0
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::UseBuffPotion.tick(&mut ctx), BtResult::Failure);
        assert!(mock.use_item_log.borrow().is_empty());
    }

    #[test]
    fn use_buff_potion_fails_when_cooldown_active() {
        let mut mock = Mock11a::new();
        mock.buff_potion_id = 13452;
        mock.potion_cd_ready = false;
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::UseBuffPotion.tick(&mut ctx), BtResult::Failure);
        assert!(mock.use_item_log.borrow().is_empty());
    }

    #[test]
    fn use_utility_potion_uses_category_1() {
        let mut mock = Mock11a::new();
        mock.utility_potion_id = 5634; // Free Action Potion
        mock.potion_cd_ready = true;
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::UseUtilityPotion.tick(&mut ctx), BtResult::Success);
        let log = mock.use_item_log.borrow();
        assert_eq!(log.as_slice(), &[5634]);
    }

    #[test]
    fn use_racial_gates_on_knows_spell() {
        let mut mock = Mock11a::new();
        mock.known_spells.insert(20594); // Stoneform
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        // Bot knows Stoneform → cast succeeds.
        assert_eq!(
            Bt::UseRacial(SpellId(20594)).tick(&mut ctx),
            BtResult::Success
        );
        // Bot doesn't know Berserking → Failure.
        assert_eq!(
            Bt::UseRacial(SpellId(20554)).tick(&mut ctx),
            BtResult::Failure
        );
    }

    #[test]
    fn use_trinket_delegates_to_interface() {
        let mut mock = Mock11a::new();
        mock.use_trinket_result = true;
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::UseTrinket(0).tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::UseTrinket(1).tick(&mut ctx), BtResult::Success);
        let log = mock.use_trinket_log.borrow();
        assert_eq!(log.as_slice(), &[0, 1]);
    }

    #[test]
    fn use_trinket_fails_when_interface_refuses() {
        let mock = Mock11a::new(); // use_trinket_result = false
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        assert_eq!(Bt::UseTrinket(0).tick(&mut ctx), BtResult::Failure);
    }

    // ── 11j: world interaction / economy actions ────────────────────────

    #[test]
    fn world_interaction_actions_with_default_mock() {
        let mut owned = TestCtxOwned::new();
        let mut ctx = owned.ctx();
        // Actions that delegate to FFI and the mock returns false by default:
        assert_eq!(Bt::Gossip(123).tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::BuyFromVendor(ItemId(100), 1).tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::MailItem.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::CheckMail.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::BankDeposit.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::BankWithdraw.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::AhPost.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::AhBid.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::LootRoll.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::AutoLootRoll.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::ShareQuest.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::EquipItem(ItemId(100)).tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::UnequipSlot(0).tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::ApplyOutfit.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::Fish.tick(&mut ctx), BtResult::Failure);
        // RandomEmote: mock do_text_emote returns false (default)
        assert_eq!(Bt::RandomEmote.tick(&mut ctx), BtResult::Failure);
        // RandomSay: mock say returns true (overridden in test mock)
        assert_eq!(Bt::RandomSay.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::WorldBuffTravel(SpellId(1)).tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::RtscConsumeMoveQueue.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::LfgJoin.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::LfgAccept.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::AcceptBgInvite.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::QueueBg.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::DefendBase.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::CaptureFlag.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::ReturnFlag.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::AssaultBase.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::ArenaEngageSetup.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::ArenaPeel.tick(&mut ctx), BtResult::Failure);
        // DungeonStayNearTank: tank position is {0,0,0} so returns Failure
        assert_eq!(Bt::DungeonStayNearTank.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::DungeonAvoidBreakingCc.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::DebugDumpState(0).tick(&mut ctx), BtResult::Failure);
    }

    // ── 11i: social / group actions ────────────────────────────────────

    #[test]
    fn social_actions_delegate_to_interface() {
        let mut mock = Mock11a::new();
        mock.accept_group_invite_result = true;
        mock.leave_group_result = true;
        mock.accept_ready_check_result = true;
        mock.accept_trade_result = true;
        mock.accept_duel_result = true;
        mock.decline_duel_result = true;
        mock.accept_summon_result = true;
        mock.use_meeting_stone_result = true;
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(Bt::AcceptGroupInvite.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::LeaveGroup.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::AcceptReadyCheck.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::AcceptTradeRequest.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::AcceptDuelRequest.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::DeclineDuelRequest.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::AcceptSummon.tick(&mut ctx), BtResult::Success);
        assert_eq!(Bt::UseMeetingStone.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn social_actions_fail_when_interface_refuses() {
        let mock = Mock11a::new(); // all results default to false
        let mut owned = TestCtxOwned::new();
        let mut ctx = ctx_with_iface(&mut owned, &mock);

        assert_eq!(Bt::AcceptGroupInvite.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::LeaveGroup.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::AcceptReadyCheck.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::AcceptTradeRequest.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::AcceptDuelRequest.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::DeclineDuelRequest.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::AcceptSummon.tick(&mut ctx), BtResult::Failure);
        assert_eq!(Bt::UseMeetingStone.tick(&mut ctx), BtResult::Failure);
    }

    // ── Trainer / talents ─────────────────────────────────────────────

    #[test]
    fn learn_trainer_spells_always_succeeds() {
        let mut owned = TestCtxOwned::new();
        let mut ctx = owned.ctx();
        assert_eq!(Bt::LearnTrainerSpells.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn apply_talent_build_fails_with_no_free_points() {
        let mut owned = TestCtxOwned::new();
        let mut ctx = owned.ctx();
        // Mock defaults: bot_free_talent_points = 0
        assert_eq!(Bt::ApplyTalentBuild.tick(&mut ctx), BtResult::Failure);
    }

    // ── Step 13: cross-class reactive combat ──────────────────────────

    #[test]
    fn pull_back_fails_without_follow_target() {
        let mut owned = TestCtxOwned::new();
        // No group members, no master → Failure.
        assert_eq!(Bt::PullBack.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn pull_back_returns_running_when_far_from_target() {
        let mock = Mock11a::new();
        let mut owned = TestCtxOwned::new();
        // Add a group member so pick_follow_target resolves.
        owned.snap.group_members[0] = 999;
        owned.snap.group_size = 1;
        let mut ctx = ctx_with_iface(&mut owned, &mock);
        // NullInterface/Mock11a returns default distance (> 5.0) and
        // follow returns false → Failure (no movement started).
        // With Mock11a the default unit_distance is 0.0 (or 10.0 depending
        // on mock). We need the mock to return distance > 5 for the
        // Running path. Since Mock11a wraps NullInterface which returns
        // 10.0 by default, follow returns true → Running.
        let result = Bt::PullBack.tick(&mut ctx);
        // The mock's unit_distance returns 10.0 and follow returns true.
        assert_eq!(result, BtResult::Running);
    }

    #[test]
    fn wait_for_attack_fails_without_target() {
        let mut owned = TestCtxOwned::new();
        assert_eq!(Bt::WaitForAttack.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn preheal_and_heal_interrupt_are_stubs() {
        let mut owned = TestCtxOwned::new();
        assert_eq!(Bt::PreHeal.tick(&mut owned.ctx()), BtResult::Failure);
        assert_eq!(Bt::HealInterrupt.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn throttle_enforces_cooldown_after_success() {
        // A child that always returns Success must still be throttled once
        // the cooldown has been armed.
        let tree = Bt::throttle(10_000, Bt::HoldPosition); // HoldPosition returns Success.
        let mut owned = TestCtxOwned::new();

        owned.time_ms = 10_000;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success);

        // Within the cooldown window — throttle must inhibit.
        owned.time_ms = 15_000;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);

        // After cooldown — child fires again.
        owned.time_ms = 21_000;
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success);
    }
}
