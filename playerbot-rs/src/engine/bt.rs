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
use crate::bot::settings::{BehaviorMode, CombatOrder, Reactivity, StrategyFlags};
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
    /// Bot is currently in combat.
    InCombat,
    /// Bot is alive (not dead/ghost).
    IsAlive,
    /// Bot is mounted.
    IsMounted,
    /// Bot is indoors.
    IsIndoor,
    /// Bot is currently moving.
    IsMoving,
    /// Bot's current behavior mode matches.
    ModeIs(BehaviorMode),
    /// Bot's combat order contains all of these flags.
    CombatOrderHas(CombatOrder),
    /// Bot's strategy flags contain all of these. Use to gate opt-in
    /// subtrees (RPG branches, RTSC, grind extras, CC management).
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
    /// Revive a dead pet.
    RevivePet,
    /// Summon pet if none exists.
    SummonPet,
    /// Feed unhappy pet (Hunter).
    FeedPet,
    /// Find and attack a level-appropriate mob for grinding.
    GrindTarget,

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
        match self {
            // ── Compositors ──────────────────────────────────────────────
            Bt::Seq(children) => {
                for child in children {
                    match child.tick(ctx) {
                        BtResult::Success => {}
                        other => return other,
                    }
                }
                BtResult::Success
            }
            Bt::Sel(children) => {
                for child in children {
                    match child.tick(ctx) {
                        BtResult::Failure => {}
                        other => return other,
                    }
                }
                BtResult::Failure
            }
            Bt::Not(child) => match child.tick(ctx) {
                BtResult::Success => BtResult::Failure,
                BtResult::Failure => BtResult::Success,
                other @ BtResult::Running => other,
            },
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
                result
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
            Bt::InCombat => ok(ctx.in_combat()),
            Bt::IsAlive => ok(ctx.snap.self_.is_alive),
            Bt::IsMounted => ok(ctx.interface.is_mounted()),
            Bt::IsIndoor => ok(ctx.interface.is_indoor()),
            Bt::IsMoving => ok(ctx.snap.self_.is_moving),
            Bt::ModeIs(mode) => ok(ctx.settings.mode == *mode),
            Bt::CombatOrderHas(flags) => ok(ctx.settings.combat_order.contains(*flags)),
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
            Bt::ShouldEngage => ok(match ctx.settings.reactivity {
                Reactivity::Passive => false,
                Reactivity::Defensive => !ctx.attackers.is_empty(),
                Reactivity::Aggressive => !ctx.attackers.is_empty() || !ctx.nearby.is_empty(),
            }),
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
                let too_close = ctx
                    .current_target()
                    .is_some_and(|t| ctx.interface.unit_distance(t) < *min_range);
                if !too_close {
                    return BtResult::Failure;
                }
                move_to_safe(ctx, *min_range * 2.0)
            }
            Bt::MoveBehind(distance) => {
                let target = match ctx.current_target() {
                    Some(t) => t,
                    None => return BtResult::Failure,
                };
                let pos = ctx.interface.get_behind_position(target, *distance);
                if ctx.interface.move_to(pos.x, pos.y, pos.z) {
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
                let target = match ctx.current_target() {
                    Some(t) => t,
                    None => return BtResult::Failure,
                };
                if ctx.interface.unit_distance(target) <= *range {
                    return BtResult::Failure;
                }
                let snap = ctx.interface.get_unit_snapshot(target);
                if ctx.interface.move_to(snap.pos.x, snap.pos.y, snap.pos.z) {
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
                    Some(&unit) if ctx.interface.attack(unit) => BtResult::Success,
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
            Bt::VendorSellGrey => tick_vendor(ctx),
            Bt::RepairEquipment => tick_repair(ctx),
            Bt::TurnInQuest => tick_turn_in_quest(ctx),
            Bt::AcceptQuests => tick_accept_quests(ctx),
            Bt::AttackQuestMob => tick_attack_quest_mob(ctx),
            Bt::TravelToBlackboard => tick_travel(ctx),
            Bt::RevivePet => {
                if ctx.interface.revive_pet() {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            Bt::SummonPet => {
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
        }
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
    if ctx.timers.gcd_active(ctx.server_time_ms) {
        return BtResult::Failure;
    }
    if ctx.timers.spell_on_cooldown(spell, ctx.server_time_ms) {
        return BtResult::Failure;
    }
    if ctx.interface.cast_spell(spell, target) {
        ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
        BtResult::Success
    } else {
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
    //
    // The "only re-follow when far enough away" gate avoids fighting the
    // movement generator on every tick — CMaNGOS already handles the
    // close-range stick-with-target case via the chase generator.

    let Some(target) = pick_follow_target(ctx) else {
        return BtResult::Failure;
    };

    if ctx.interface.unit_distance(target) <= REFOLLOW_THRESHOLD {
        // Already in chase range — don't re-issue. Sticky behavior matches
        // PB2's `tooFarDistance` gate regardless of formation type.
        return BtResult::Success;
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
        FormationOutput::Position { x, y, z } => {
            ctx.interface.move_to(x, y, z);
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
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_dispel(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some((member, _debuff_id)) = ctx.interface.find_dispellable_target() {
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
        // Rebirth works in combat, others only out of combat.
        if spell != REBIRTH && ctx.in_combat() {
            return BtResult::Failure;
        }
        if ctx.interface.can_cast(spell, dead) && ctx.interface.cast_spell(spell, dead) {
            ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
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
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn tick_focus_attack(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some(focus) = ctx.settings.focus_target
        && ctx.current_target() != Some(focus)
        && ctx.interface.attack(focus)
    {
        return BtResult::Success;
    }
    BtResult::Failure
}

fn tick_tank_pickup(ctx: &mut TickContext<'_>) -> BtResult {
    for &attacker in ctx.attackers {
        let snap = ctx.interface.get_unit_snapshot(attacker);
        if snap.current_target != ctx.bot_handle && ctx.interface.taunt(attacker) {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn tick_assist_leader(ctx: &mut TickContext<'_>) -> BtResult {
    let leader_target = ctx
        .interface
        .group_get_tank()
        .or_else(|| {
            ctx.snap.group_members[..ctx.snap.group_size as usize]
                .iter()
                .copied()
                .find(|&h| h != 0 && h != ctx.bot_handle)
        })
        .and_then(|leader| {
            let snap = ctx.interface.get_unit_snapshot(leader);
            if snap.current_target != 0 {
                Some(snap.current_target)
            } else {
                None
            }
        });

    if let Some(target) = leader_target
        && ctx.current_target() != Some(target)
        && ctx.interface.attack(target)
    {
        return BtResult::Success;
    }
    BtResult::Failure
}

fn tick_protect(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some(protect) = ctx.settings.protect_target {
        for &attacker in ctx.attackers {
            let snap = ctx.interface.get_unit_snapshot(attacker);
            if snap.current_target == protect && ctx.interface.attack(attacker) {
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
        && ctx.interface.attack(target)
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

fn tick_grind(ctx: &mut TickContext<'_>) -> BtResult {
    // Re-engage existing target.
    if let Some(t) = ctx.current_target()
        && ctx.interface.is_attackable(t)
        && ctx.interface.attack(t)
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
        && ctx.interface.attack(t)
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
        && ctx.interface.attack(enemy)
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
    use crate::ffi::BotRole;

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
