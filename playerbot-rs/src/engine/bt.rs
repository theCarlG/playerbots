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
///     Seq(vec![HasDebuff(LIVING_BOMB), MoveAwayFromRaid(40.0)]),
///     Seq(vec![Not(Box::new(InCombat)), Follow]),
/// ])
/// ```
use std::cell::Cell;

use crate::bot::settings::{BehaviorMode, CombatOrder, Reactivity, StrategyFlags};
use crate::bot::state::PlayerClass;
use crate::engine::bt_nodes::{BtNode, BtResult};
use crate::engine::context::TickContext;
use crate::ffi::SpellId;
use crate::noncombat::buffing::GroupBuff;

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
    Throttle(u64, Cell<u64>, Box<Bt>),

    // ── Conditions — encounter ───────────────────────────────────────────
    /// This bot has the specified aura/debuff on itself.
    HasDebuff(SpellId),
    /// The bot's current target has the specified aura.
    TargetHasAura(SpellId),
    /// This bot is missing the specified aura (inverse of HasDebuff on self).
    SelfMissingAura(SpellId),
    /// The current target is missing the specified aura.
    TargetMissingAura(SpellId),
    /// This bot is missing every rank in the list.
    SelfMissingAnyRank(&'static [SpellId]),
    /// The current target is missing every rank in the list.
    TargetMissingAnyRank(&'static [SpellId]),
    /// Target has fewer than `max` stacks of the specified aura (missing = 0 stacks).
    TargetAuraStacksBelow(SpellId, u8),
    /// Current target's HP is below this fraction (0.0–1.0).
    TargetHpBelow(f32),
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
    /// The current target is closer than the specified distance.
    TargetCloserThan(f32),
    /// The current target is farther than the specified distance.
    TargetFartherThan(f32),
    /// Number of hostile nearby units is at least `n`.
    NearbyAtLeast(usize),
    /// Number of attackers on this bot is at least `n`.
    AttackersAtLeast(usize),
    /// Current group size is at least `n`.
    GroupSizeAtLeast(u8),
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
    /// Bot's HP is below this fraction (0.0–1.0).
    HpBelow(f32),
    /// Bot's mana is below this fraction (0.0–1.0).
    ManaBelow(f32),
    /// Bot uses mana (not rage/energy/runic power).
    UsesMana,
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
}

impl Bt {
    pub fn not(self) -> Self {
        Bt::Not(Box::new(self))
    }

    pub fn throttle(interval_ms: u64, child: Self) -> Self {
        Bt::Throttle(interval_ms, Cell::new(0), Box::new(child))
    }

    /// Run `self` only when `guard` succeeds. Equivalent to `Seq(vec![guard, self])`.
    ///
    /// Reads top-down: `CastOnSelf(ICE_BLOCK).when(HpBelow(0.20))`.
    pub fn when(self, guard: Bt) -> Bt {
        Bt::Seq(vec![guard, self])
    }

    /// Try `self`; if it fails, fall back to `fallback`. Equivalent to `Sel(vec![self, fallback])`.
    pub fn or_else(self, fallback: Bt) -> Bt {
        Bt::Sel(vec![self, fallback])
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
            Bt::Throttle(interval_ms, last_ms, child) => {
                let now = ctx.server_time_ms;
                if now.saturating_sub(last_ms.get()) < *interval_ms {
                    return BtResult::Failure;
                }
                let result = child.tick(ctx);
                if result != BtResult::Failure {
                    last_ms.set(now);
                }
                result
            }

            // ── Conditions — encounter ───────────────────────────────────
            Bt::HasDebuff(spell) => ok(ctx.interface.has_aura(ctx.bot_handle, *spell)),
            Bt::TargetHasAura(spell) => ok(ctx
                .current_target()
                .is_some_and(|t| ctx.interface.has_aura(t, *spell))),
            Bt::SelfMissingAura(spell) => ok(!ctx.interface.has_aura(ctx.bot_handle, *spell)),
            Bt::TargetMissingAura(spell) => ok(ctx
                .current_target()
                .is_some_and(|t| !ctx.interface.has_aura(t, *spell))),
            Bt::SelfMissingAnyRank(ranks) => ok(!crate::engine::aura_helpers::has_any_rank(
                ctx.interface,
                ctx.bot_handle,
                ranks,
            )),
            Bt::TargetMissingAnyRank(ranks) => ok(ctx.current_target().is_some_and(|t| {
                !crate::engine::aura_helpers::has_any_rank(ctx.interface, t, ranks)
            })),
            Bt::TargetAuraStacksBelow(spell, max) => ok(ctx.current_target().is_some_and(|t| {
                ctx.interface
                    .get_aura(t, *spell)
                    .is_none_or(|a| a.stacks < *max)
            })),
            Bt::TargetHpBelow(pct) => ok(ctx.current_target().is_some_and(|t| {
                let s = ctx.interface.get_unit_snapshot(t);
                s.max_health > 0 && (s.health as f32 / s.max_health as f32) < *pct
            })),
            Bt::TargetIsCasting => ok(ctx
                .current_target()
                .is_some_and(|t| ctx.interface.get_unit_snapshot(t).is_casting)),
            Bt::IsClass(class) => ok(ctx.class == *class),
            Bt::IsTank => ok(ctx.is_tank()),
            Bt::IsRanged => ok(ctx.is_ranged_or_healer()),
            Bt::IsMeleeDps => ok(!ctx.is_ranged_or_healer() && !ctx.is_tank()),
            Bt::TargetCloserThan(dist) => ok(ctx
                .current_target()
                .is_some_and(|t| ctx.interface.unit_distance(t) < *dist)),
            Bt::TargetFartherThan(dist) => ok(ctx
                .current_target()
                .is_some_and(|t| ctx.interface.unit_distance(t) > *dist)),
            Bt::NearbyAtLeast(n) => ok(ctx.nearby.len() >= *n),
            Bt::AttackersAtLeast(n) => ok(ctx.attackers.len() >= *n),
            Bt::GroupSizeAtLeast(n) => ok(ctx.snap.group_size >= *n),
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
            Bt::StrategyEnabled(flags) => ok(ctx.settings.strategies.contains(*flags)),
            Bt::ReactivityIs(r) => ok(ctx.settings.reactivity == *r),
            Bt::HpBelow(pct) => ok(ctx.self_hp_pct() < *pct),
            Bt::ManaBelow(pct) => ok(ctx.self_mana_pct() < *pct),
            Bt::UsesMana => ok(ctx.snap.self_.power_type == 0),
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

const FOLLOW_DIST: f32 = 3.0;
const REFOLLOW_THRESHOLD: f32 = 8.0;

fn tick_follow(ctx: &mut TickContext<'_>) -> BtResult {
    // Prefer following the designated tank.
    if let Some(tank) = ctx.interface.group_get_tank() {
        if ctx.interface.unit_distance(tank) > REFOLLOW_THRESHOLD {
            if ctx.interface.follow(tank, FOLLOW_DIST, 0.0) {
                return BtResult::Success;
            }
        }
    }
    // Fall back to any group member far enough away.
    let member = ctx.snap.group_members[..ctx.snap.group_size as usize]
        .iter()
        .copied()
        .find(|&h| {
            h != 0 && h != ctx.bot_handle && ctx.interface.unit_distance(h) > REFOLLOW_THRESHOLD
        });
    if let Some(target) = member {
        if ctx.interface.follow(target, FOLLOW_DIST, 0.0) {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

// ── Guard return ────────────────────────────────────────────────────────────

const GUARD_LEASH_DIST: f32 = 5.0;

fn tick_guard_return(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some((gx, gy, gz)) = ctx.settings.guard_position {
        let pos = &ctx.snap.self_.pos;
        let dx = pos.x - gx;
        let dy = pos.y - gy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > GUARD_LEASH_DIST {
            if ctx.interface.move_to(gx, gy, gz) {
                return BtResult::Running;
            }
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
        if let Some(target_handle) = find_buff_target(ctx, buff) {
            if ctx.interface.cast_spell(buff.spell_id, target_handle) {
                ctx.timers.on_spell_cast(buff.spell_id, ctx.server_time_ms);
                return BtResult::Success;
            }
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
    if let Some(focus) = ctx.settings.focus_target {
        if ctx.current_target() != Some(focus) && ctx.interface.attack(focus) {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn tick_tank_pickup(ctx: &mut TickContext<'_>) -> BtResult {
    for &attacker in ctx.attackers {
        let snap = ctx.interface.get_unit_snapshot(attacker);
        if snap.current_target != ctx.bot_handle {
            if ctx.interface.taunt(attacker) {
                return BtResult::Success;
            }
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

    if let Some(target) = leader_target {
        if ctx.current_target() != Some(target) && ctx.interface.attack(target) {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn tick_protect(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some(protect) = ctx.settings.protect_target {
        for &attacker in ctx.attackers {
            let snap = ctx.interface.get_unit_snapshot(attacker);
            if snap.current_target == protect {
                if ctx.interface.attack(attacker) {
                    return BtResult::Success;
                }
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
    if has_active {
        if let Some(&target) = ctx.nearby.iter().find(|&&u| ctx.interface.is_attackable(u)) {
            if ctx.interface.attack(target) {
                return BtResult::Success;
            }
        }
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
    if let Some(t) = ctx.current_target() {
        if ctx.interface.is_attackable(t) && ctx.interface.attack(t) {
            return BtResult::Success;
        }
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
    if let Some(t) = target {
        if ctx.interface.attack(t) {
            return BtResult::Success;
        }
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
    if let Some(&enemy) = enemies.first() {
        if ctx.interface.attack(enemy) {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

// ── RPG helpers ─────────────────────────────────────────────────────────────

fn tick_rpg_wander(ctx: &mut TickContext<'_>) -> BtResult {
    if let Some(pos) = ctx.interface.get_random_point_nearby(20.0) {
        if ctx.interface.move_to(pos.x, pos.y, pos.z) {
            return BtResult::Running;
        }
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
    use crate::engine::context::tests::{TestCtxOwned, TestInterface, make_encounter_ctx};
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
        use Bt::*;
        let spell = SpellId(12345);
        let tree = HasDebuff(spell);
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
}
