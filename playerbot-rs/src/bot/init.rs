use crate::{Sel, Seq};
/// Bot initialization — builds per-FSM behavior trees from (class, spec).
use crate::{
    bot::settings::{BehaviorMode, BotStateKind, Reactivity, StrategyFlags, StrategySet},
    bot::state::{BotState, BotTrees, PlayerClass, PlayerSpec},
    classes,
    combat::reactive,
    engine::{bt::Bt, macro_fsm::ActiveFsm},
    ffi::interface::BotInterface,
    noncombat::GroupBuff,
    world,
};

/// Build a `BotState` from its handle, interface, class, and spec.
pub fn create_bot(
    handle: u64,
    interface: Box<dyn BotInterface>,
    class: PlayerClass,
    spec: PlayerSpec,
) -> Box<BotState> {
    let role = spec.default_role();
    let trees = build_bot_trees(class, spec);
    let mut state = BotState::new(handle, interface, class, spec, role, trees);
    // Layer PB2's per-class default strategies on top of the global
    // `PlayerbotAIConfig.cpp` baseline (empty combat/react/dead slots +
    // `+return,+delayed roll` in nonCombat). This matches PB2's
    // `AiFactory::Add*Strategies` runtime composition — see `pb2_kit_strategies`
    // for the per-class breakdown ported from `AiFactory.cpp`.
    let kit = kit_strategies(class, spec);
    state.settings.strategies = kit;
    state.settings.init_strategies = kit;
    // Tanks default to marking targets with skull (icon 7). Non-tanks
    // leave it as None so the MarkRtiPreferred node no-ops.
    let combat_flags = state.settings.strategies.get(BotStateKind::Combat);
    if combat_flags.contains(StrategyFlags::TANK_ASSIST) {
        state.settings.preferred_rti_icon = Some(7);
    }
    // Set default position_stance to match the kitted strategy flags
    // so `stance ?` reports the correct initial state.
    use crate::bot::settings::PositionStance;
    if combat_flags.contains(StrategyFlags::BEHIND) && combat_flags.contains(StrategyFlags::CLOSE) {
        state.settings.position_stance = PositionStance::Turnback;
    } else if combat_flags.contains(StrategyFlags::BEHIND) {
        state.settings.position_stance = PositionStance::Behind;
    } else if combat_flags.contains(StrategyFlags::CLOSE) {
        state.settings.position_stance = PositionStance::Tank;
    }
    Box::new(state)
}

/// PB2 `AiFactory::AddDefaultCombatStrategies` + `AddDefaultNonCombatStrategies`
/// + per-class `AddClassDefaultCombatStrategies` composition.
///
/// Returns a `StrategySet` with the global baseline from
/// `PlayerbotAIConfig.cpp` plus every strategy name the PB2 kit for
/// `(class, spec)` turns on by default. This is the data side of
/// `PARITY_PLAN` §3.2.
///
/// These flags are the single source of truth for both chat-filter/query
/// responses and BT runtime gating. The old `CombatOrder` bitfield has
/// been removed; `co` commands now read/write the Combat strategy slot
/// directly.
pub fn kit_strategies(class: PlayerClass, spec: PlayerSpec) -> StrategySet {
    use BotStateKind::{Combat, NonCombat};
    use StrategyFlags as F;

    // Start from the global PB2 baseline (empty Combat/Reaction/Dead +
    // NonCombat = RETURN | DELAYED_ROLL).
    let mut set = StrategySet::pb2_defaults();

    // ── All-bot combat base — PB2 `AiFactory::AddDefaultCombatStrategies`.
    let combat_base =
        F::MOUNT | F::AVOID_MOBS | F::RACIALS | F::DEFAULT | F::DUEL | F::PVP | F::ASSIST;
    *set.get_mut(Combat) = set.get(Combat) | combat_base;

    // ── All-bot non-combat base — PB2 `AddDefaultNonCombatStrategies`.
    let noncombat_base = F::AVOID_MOBS | F::WBUFF;
    *set.get_mut(NonCombat) = set.get(NonCombat) | noncombat_base;

    // ── Per-class/spec combat additions — delegated to class modules.
    *set.get_mut(Combat) = set.get(Combat) | class_strategies(class, spec);

    set
}

/// Dispatch to the class module's `default_strategies` for class-specific
/// combat flags. Returns `StrategyFlags::NONE` for mismatched (class, spec).
fn class_strategies(class: PlayerClass, spec: PlayerSpec) -> StrategyFlags {
    use PlayerClass::{
        DeathKnight, Druid, Hunter, Mage, Paladin, Priest, Rogue, Shaman, Warlock, Warrior,
    };
    match class {
        Warrior => classes::warrior::default_strategies(spec),
        Paladin => classes::paladin::default_strategies(spec),
        Priest => classes::priest::default_strategies(spec),
        Druid => classes::druid::default_strategies(spec),
        Hunter => classes::hunter::default_strategies(spec),
        Mage => classes::mage::default_strategies(spec),
        Rogue => classes::rogue::default_strategies(spec),
        Shaman => classes::shaman::default_strategies(spec),
        Warlock => classes::warlock::default_strategies(spec),
        DeathKnight => classes::deathknight::default_strategies(spec),
    }
}

/// Try to derive a `PlayerSpec` from a strategy flag set. Used when the
/// `MangosBot` addon sends `co +protection,?` to override the bot's spec at
/// runtime. Returns `None` when no spec flag is present or the flag doesn't
/// match the bot's class.
pub fn spec_from_strategy_flags(class: PlayerClass, flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerClass::{
        DeathKnight, Druid, Hunter, Mage, Paladin, Priest, Rogue, Shaman, Warlock, Warrior,
    };
    match class {
        Warrior => classes::warrior::spec_from_flags(flags),
        Paladin => classes::paladin::spec_from_flags(flags),
        Priest => classes::priest::spec_from_flags(flags),
        Druid => classes::druid::spec_from_flags(flags),
        Hunter => classes::hunter::spec_from_flags(flags),
        Mage => classes::mage::spec_from_flags(flags),
        Rogue => classes::rogue::spec_from_flags(flags),
        Shaman => classes::shaman::spec_from_flags(flags),
        Warlock => classes::warlock::spec_from_flags(flags),
        DeathKnight => classes::deathknight::spec_from_flags(flags),
    }
}

/// Rebuild the bot's behavior tree and strategies for a new spec.
/// Called when the `MangosBot` addon sends a spec strategy flag toggle
/// (e.g. `co +protection` on a warrior).
pub fn rebuild_for_spec(bot: &mut BotState, new_spec: PlayerSpec) {
    if bot.spec == new_spec {
        return;
    }
    bot.spec = new_spec;
    bot.role = new_spec.default_role();
    bot.trees = build_bot_trees(bot.class, new_spec);
    let kit = kit_strategies(bot.class, new_spec);
    bot.settings.strategies = kit;
    bot.settings.init_strategies = kit;
    bot.settings.class_prefs =
        crate::bot::class_prefs::ClassPrefs::default_for(bot.class, new_spec);
    bot.reset_strategies();
}

/// Build the class behavior tree for a given (class, fsm, spec).
///
/// `ActiveFsm` is a first-class parameter — each class module's
/// `build_tree(fsm, spec)` receives it directly and dispatches to the
/// appropriate spec tree. This is the `fn(state) -> Bt` pattern from the
/// architecture plan.
fn class_bt(class: PlayerClass, fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerClass::{
        DeathKnight, Druid, Hunter, Mage, Paladin, Priest, Rogue, Shaman, Warlock, Warrior,
    };
    match class {
        Warrior => classes::warrior::build_tree(fsm, spec),
        Paladin => classes::paladin::build_tree(fsm, spec),
        Priest => classes::priest::build_tree(fsm, spec),
        Druid => classes::druid::build_tree(fsm, spec),
        Hunter => classes::hunter::build_tree(fsm, spec),
        Mage => classes::mage::build_tree(fsm, spec),
        Rogue => classes::rogue::build_tree(fsm, spec),
        Shaman => classes::shaman::build_tree(fsm, spec),
        Warlock => classes::warlock::build_tree(fsm, spec),
        DeathKnight => classes::deathknight::build_tree(fsm, spec),
    }
}

/// Look up the persistent group buffs for this (class, spec).
fn class_buffs(class: PlayerClass, spec: PlayerSpec) -> &'static [GroupBuff] {
    use PlayerClass::{
        DeathKnight, Druid, Hunter, Mage, Paladin, Priest, Rogue, Shaman, Warlock, Warrior,
    };
    match class {
        Warrior => classes::warrior::buffs(spec),
        Paladin => classes::paladin::buffs(spec),
        Priest => classes::priest::buffs(spec),
        Druid => classes::druid::buffs(spec),
        Hunter => classes::hunter::buffs(spec),
        Mage => classes::mage::buffs(spec),
        Rogue => classes::rogue::buffs(spec),
        Shaman => classes::shaman::buffs(spec),
        Warlock => classes::warlock::buffs(spec),
        DeathKnight => classes::deathknight::buffs(spec),
    }
}

/// Build per-FSM behavior trees for a given class/spec.
///
/// Returns a `BotTrees` with separate trees for each `ActiveFsm` state.
/// The tick loop selects which tree to run based on the current FSM state,
/// with encounter override checked first (at tick-time, not in the tree).
fn build_bot_trees(class: PlayerClass, spec: PlayerSpec) -> BotTrees {
    let buffs = class_buffs(class, spec);
    let class_combat = class_bt(class, ActiveFsm::Combat, spec);

    BotTrees {
        combat: build_combat_tree(class_combat.clone()),
        world: build_world_tree(class_combat),
        dead: world::death::death_subtree(),
        maintenance: maintenance_subtree(buffs),
    }
}

/// Build the combat FSM tree: reactive subtrees + class rotation.
///
/// Encounter override is NOT in this tree — it's handled at tick-level
/// dispatch so encounters can inject behavior in any FSM state.
fn build_combat_tree(class_rotation: Bt) -> Bt {
    use Bt::{ModeIs, ShouldEngage};

    Sel!(
        // Passive — actively disengage: stop auto-attack and movement
        // so previously-engaged combat doesn't keep swinging. Checks
        // both BehaviorMode::Passive (addon strategy toggle) and
        // Reactivity::Passive (`react passive` command).
        Seq!(
            Sel!(
                ModeIs(BehaviorMode::Passive),
                Bt::ReactivityIs(Reactivity::Passive),
            ),
            Bt::Disengage,
        ),
        // Duel request handling.
        Seq!(
            Bt::DuelRequested,
            Sel!(
                Seq!(
                    Bt::StrategyEnabled(StrategyFlags::DUEL),
                    Bt::AcceptDuelRequest
                ),
                Bt::DeclineDuelRequest,
            ),
        ),
        // Combat wrapper: reactive + rotation.
        // Also fires when group members are injured (healers top off OOC),
        // when dueling, or when commanded to focus a target.
        Seq!(
            Sel!(
                Bt::InCombat,
                ShouldEngage,
                Bt::InDuel,
                Bt::GroupMembersBelow(1, 0.90),
                Bt::HasFocusTarget
            ),
            combat_wrapper(class_rotation),
        ),
        Bt::Noop,
    )
}

/// Build the world (out-of-combat) FSM tree: eat/drink + mode dispatch.
///
/// `class_rotation` is the same class combat rotation used in the combat
/// tree. In the world tree it's gated behind `GroupMembersBelow` so healer
/// specs can top off injured party members OOC. Non-healer rotations
/// (which only have damage nodes) will simply return Failure when no
/// enemy target exists, falling through harmlessly.
fn build_world_tree(class_rotation: Bt) -> Bt {
    use Bt::Consumables;

    Sel!(
        // On taxi — do nothing.
        Seq!(Bt::OnTaxi, Bt::Noop),
        // Apply missing world buffs (configured in server config).
        crate::strategies::wbuff::build(),
        // Resurrect dead party members before eating/drinking.
        reactive::resurrect_subtree(),
        // OOC healing — healers top off injured group members.
        // Runs before Consumables so the healer heals first, then drinks.
        Seq!(
            Bt::GroupMembersBelow(1, 0.90),
            Bt::throttle(1_000, class_rotation),
        ),
        // Eat/drink to recover HP/mana.
        Consumables,
        // Duel request handling (can happen OOC too).
        Seq!(
            Bt::DuelRequested,
            Sel!(
                Seq!(
                    Bt::StrategyEnabled(StrategyFlags::DUEL),
                    Bt::AcceptDuelRequest
                ),
                Bt::DeclineDuelRequest,
            ),
        ),
        // Mode dispatch — follow, grind, quest, etc.
        mode_dispatch(),
        Bt::Noop,
    )
}

/// Wrap a class rotation in the shared reactive subtrees that apply to
/// every class.
///
/// Structure:
///   A) **Reactive Sel** — high-priority behaviors that short-circuit the
///      rest (flee, interrupt, dispel, rez, threat, pull-back, pre-heal).
///      Any one of these firing replaces the normal rotation for that tick.
///   B) **Combat action Sel** — targeting+positioning, then rotation.
///      Targeting is *optional* (wrapped in `Sel(targeting, Noop)`) so
///      healers and other support specs can reach their rotation even
///      without an enemy target. When targeting succeeds the bot also
///      gets positioning (close/ranged/behind/kite). When it fails
///      the bot still proceeds to the rotation for healing, buffing, etc.
fn combat_wrapper(class_rotation: Bt) -> Bt {
    use BehaviorMode;
    use Bt::{IsTank, MaintainConfiguredCurse, ModeIs};
    Sel!(
        // ── A) Reactive — any one short-circuits the rest ────────────
        reactive::flee_subtree(),
        reactive::interrupt_subtree(),
        reactive::heal_interrupt_subtree(),
        reactive::dispel_subtree(),
        reactive::resurrect_subtree(),
        reactive::threat_subtree(),
        reactive::pull_back_subtree(),
        reactive::preheal_subtree(),
        // CC adds: cast crowd control on RTI CC target or nearest add.
        // Throttled inside AutoCc to prevent spam.
        Bt::throttle(2_000, Bt::AutoCc),
        // ── B) Combat pipeline ───────────────────────────────────────
        //
        // Three-step Seq: target → position → act. Each step uses
        // `Optional` or `Sel(…, Noop)` to always return Success so the
        // Seq continues to the next step regardless.
        //
        // This ensures the class rotation always gets a chance to run.
        // Positioning nodes fire-and-forget movement commands; the
        // rotation will naturally fail when out of range (melee at 30y)
        // but succeeds once the bot arrives.
        Seq!(
            // B.1 — Targeting + engage. Gated on ShouldEngage (which
            //        respects Reactivity for ALL roles) or an explicit focus
            //        command. Healers entering via GroupMembersBelow skip
            //        this so they don't auto-attack.
            Bt::Optional(Box::new(Seq!(
                Sel!(Bt::ShouldEngage, Bt::HasFocusTarget),
                Sel!(reactive::targeting_subtree(), Bt::Noop),
                Bt::EngageTarget,
                Sel!(reactive::mark_rti_subtree(), Bt::Noop),
            ))),
            // B.2 — Positioning (fire-and-forget). ONE movement system per
            //        bot type to avoid conflicting chase commands.
            //
            //        BEHIND: chase(target, 2, PI) via MoveBehind — handles
            //          both approach AND behind positioning in one command.
            //        CLOSE (not BEHIND): chase(target, 5, 0) via
            //          StickToTarget — melee approach + facing.
            //        RANGED: MaintainRange retreat + CloseToTarget chase.
            //        NONE of the above: FaceTarget for casters/healers.
            //
            //        Only ONE of these fires per tick. Suppressed in Stay.
            Bt::Optional(Box::new(Seq!(
                ModeIs(BehaviorMode::Stay).not(),
                Sel!(
                    // BEHIND melee (rogues, feral cat): position behind.
                    // MoveBehind returns Running while moving, Failure
                    // when already behind + in range. FaceTarget ensures
                    // the bot faces its target when already in position.
                    Seq!(
                        Bt::StrategyEnabled(StrategyFlags::BEHIND),
                        Sel!(reactive::behind_subtree(), Bt::StickToTarget(5.0), Bt::FaceTarget),
                    ),
                    // CLOSE melee (warriors, paladins, enh shaman, feral
                    // bear): approach from front. Re-issued every tick so
                    // the bot keeps tracking after spell casts. FaceTarget
                    // fires when already in melee range.
                    Seq!(
                        Bt::StrategyEnabled(StrategyFlags::CLOSE),
                        Sel!(Bt::StickToTarget(5.0), Bt::FaceTarget),
                    ),
                    // RANGED: keep distance + close if too far.
                    Seq!(
                        Bt::StrategyEnabled(StrategyFlags::RANGED),
                        Sel!(reactive::ranged_subtree(), Bt::FaceTarget),
                    ),
                    // Fallback (healers etc): just face the target.
                    Bt::FaceTarget,
                ),
            ))),
            // B.4 — Class rotation (the main event). Always runs so healers
            //        can heal OOC and DPS can use their rotation in combat.
            //        Non-tank DPS pause when about to pull aggro (>90% of
            //        tank threat) — the reactive threat_subtree handles
            //        classes with dumps (Fade, Feign, Vanish); for everyone
            //        else, skipping the rotation for one tick lets natural
            //        threat decay work.
            Sel!(
                Seq!(IsTank.not(), Bt::PullingAggro, Bt::Noop),
                Bt::throttle(2_000, MaintainConfiguredCurse),
                class_rotation,
                Bt::Noop,
            ),
        ),
    )
}

/// Mode dispatch — each behavior mode gets its own subtree.
fn mode_dispatch() -> Bt {
    use Bt::{Follow, ModeIs, StrategyEnabled};
    Sel!(
        // Follow mode — try to follow (tank → master → group member).
        // `Follow` returns Success whenever a follow target exists, even
        // if the bot is already close enough to stay put; only when the
        // bot is genuinely solo and masterless does it return Failure and
        // fall through to RPG/Grind so unclaimed random bots still do
        // something. This matches PB2's semantics and prevents grouped
        // bots from wandering off to grind while standing next to their
        // master ("restless" bug).
        //
        // Not throttled: `tick_follow` already gates the actual re-follow
        // call on `unit_distance > REFOLLOW_THRESHOLD`, so running it
        // every tick is cheap and avoids leaving a Follow-mode bot in a
        // gap where the throttle had started cooling and the selector
        // would otherwise route into RPG/Grind.
        Seq!(
            ModeIs(BehaviorMode::Follow),
            Sel!(
                Follow,
                crate::strategies::travel::build(),
                world::rpg::rpg_subtree(),
                world::grind::grind_subtree(),
            ),
        ),
        Seq!(ModeIs(BehaviorMode::Stay), world::stay::stay_subtree(),),
        Seq!(ModeIs(BehaviorMode::Grind), world::grind::grind_subtree(),),
        Seq!(ModeIs(BehaviorMode::Quest), world::quest::quest_subtree(),),
        Seq!(ModeIs(BehaviorMode::Guard), world::guard::guard_subtree(),),
        // RPG mode is only active if the RPG strategy flag is set; without
        // it, rpg-mode bots just idle-follow instead of wandering into NPCs.
        Seq!(
            ModeIs(BehaviorMode::Rpg),
            StrategyEnabled(StrategyFlags::RPG),
            world::rpg::rpg_subtree(),
        ),
        Seq!(ModeIs(BehaviorMode::Bg), world::bg::bg_subtree()),
    )
}

/// Maintenance subtree — low-priority upkeep in any non-passive mode.
fn maintenance_subtree(buffs: &'static [GroupBuff]) -> Bt {
    use Bt::{
        ApplyConfiguredBlessings, ApplyShamanImbues, Buff, EnforceWarriorStance, Follow, InCombat,
        MaintainHunterAspect, MaintainPaladinAura,
    };
    Sel!(
        Seq!(InCombat.not(), Bt::throttle(5_000, Buff(buffs))),
        // Class-prefs upkeep: each handler self-filters by `ClassPrefs`
        // variant, so only the one matching the bot's class ever does
        // work. Safe to run unconditionally.
        Bt::throttle(2_000, MaintainPaladinAura),
        Bt::throttle(2_000, MaintainHunterAspect),
        Bt::throttle(1_000, EnforceWarriorStance),
        Seq!(
            InCombat.not(),
            Bt::throttle(5_000, ApplyConfiguredBlessings),
        ),
        Seq!(InCombat.not(), Bt::throttle(30_000, ApplyShamanImbues),),
        // Auto-roll on pending loot items (delayed roll strategy).
        Bt::throttle(2_000, Bt::AutoLootRoll),
        // Check mail when near a mailbox.
        Bt::throttle(10_000, Bt::CheckMail),
        // Learn level-appropriate spells and apply talent build.
        Bt::throttle(60_000, Bt::LearnTrainerSpells),
        Bt::throttle(60_000, Bt::ApplyTalentBuild),
        world::pet::pet_subtree(),
        world::loot::loot_subtree(),
        world::gather::gather_subtree(),
        world::mount::mount_subtree(),
        world::vendor::vendor_subtree(),
        world::repair::repair_subtree(),
        // Follow as absolute fallback — but not during combat FSM (would
        // fight with combat positioning, causing the bot to ping-pong
        // between following master and closing to target). Uses
        // InCombatFsm (not InCombat) because the server combat flag can
        // briefly drop while the bot still has attackers.
        Seq!(Bt::InCombatFsm.not(), Bt::throttle(2_000, Follow)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::settings::{BotStateKind, StrategyFlags as F};

    /// Every class/spec kit must at minimum include the all-bot combat
    /// base from PB2 `AiFactory::AddDefaultCombatStrategies`.
    #[test]
    fn every_kit_has_allbot_combat_base() {
        let combos: &[(PlayerClass, PlayerSpec)] = &[
            (PlayerClass::Warrior, PlayerSpec::WarriorProtection),
            (PlayerClass::Warrior, PlayerSpec::WarriorArms),
            (PlayerClass::Warrior, PlayerSpec::WarriorFury),
            (PlayerClass::Paladin, PlayerSpec::PaladinHoly),
            (PlayerClass::Paladin, PlayerSpec::PaladinProtection),
            (PlayerClass::Paladin, PlayerSpec::PaladinRetribution),
            (PlayerClass::Priest, PlayerSpec::PriestHoly),
            (PlayerClass::Priest, PlayerSpec::PriestDiscipline),
            (PlayerClass::Priest, PlayerSpec::PriestShadow),
            (PlayerClass::Druid, PlayerSpec::DruidBalance),
            (PlayerClass::Druid, PlayerSpec::DruidFeral),
            (PlayerClass::Druid, PlayerSpec::DruidRestoration),
            (PlayerClass::Hunter, PlayerSpec::HunterBeastMastery),
            (PlayerClass::Hunter, PlayerSpec::HunterMarksmanship),
            (PlayerClass::Hunter, PlayerSpec::HunterSurvival),
            (PlayerClass::Mage, PlayerSpec::MageArcane),
            (PlayerClass::Mage, PlayerSpec::MageFire),
            (PlayerClass::Mage, PlayerSpec::MageFrost),
            (PlayerClass::Rogue, PlayerSpec::RogueAssassination),
            (PlayerClass::Rogue, PlayerSpec::RogueCombat),
            (PlayerClass::Rogue, PlayerSpec::RogueSubtlety),
            (PlayerClass::Shaman, PlayerSpec::ShamanElemental),
            (PlayerClass::Shaman, PlayerSpec::ShamanEnhancement),
            (PlayerClass::Shaman, PlayerSpec::ShamanRestoration),
            (PlayerClass::Warlock, PlayerSpec::WarlockAffliction),
            (PlayerClass::Warlock, PlayerSpec::WarlockDemonology),
            (PlayerClass::Warlock, PlayerSpec::WarlockDestruction),
            (PlayerClass::DeathKnight, PlayerSpec::DeathKnightBlood),
            (PlayerClass::DeathKnight, PlayerSpec::DeathKnightFrost),
            (PlayerClass::DeathKnight, PlayerSpec::DeathKnightUnholy),
        ];
        let base = F::MOUNT | F::AVOID_MOBS | F::RACIALS | F::DEFAULT | F::DUEL | F::PVP;
        for (c, s) in combos {
            let set = kit_strategies(*c, *s);
            assert!(
                set.has(BotStateKind::Combat, base),
                "{:?}/{:?} missing all-bot combat base",
                c,
                s
            );
            // Non-combat base: avoid mobs + wbuff, plus PB2 defaults
            // (return + delayed roll).
            let nc_base = F::AVOID_MOBS | F::WBUFF | F::RETURN | F::DELAYED_ROLL;
            assert!(
                set.has(BotStateKind::NonCombat, nc_base),
                "{:?}/{:?} missing non-combat base",
                c,
                s
            );
        }
    }

    /// Spot-check warrior kits against PB2 AiFactory.cpp §3.2 exactly.
    #[test]
    fn warrior_kit_strategies_match_pb2() {
        let prot = kit_strategies(PlayerClass::Warrior, PlayerSpec::WarriorProtection);
        let combat = prot.get(BotStateKind::Combat);
        for f in [
            F::PROTECTION,
            F::TANK_ASSIST,
            F::PULL,
            F::PULL_BACK,
            F::CLOSE,
            F::AOE,
            F::CC,
            F::BUFF,
            F::BOOST,
            F::MOUNT,
            F::AVOID_MOBS,
            F::RACIALS,
            F::DEFAULT,
            F::DUEL,
            F::PVP,
        ] {
            assert!(combat.contains(f), "prot warrior missing {:?}", f);
        }

        let arms = kit_strategies(PlayerClass::Warrior, PlayerSpec::WarriorArms);
        for f in [
            F::ARMS,
            F::DPS_ASSIST,
            F::BEHIND,
            F::AOE,
            F::CC,
            F::BUFF,
            F::BOOST,
        ] {
            assert!(
                arms.get(BotStateKind::Combat).contains(f),
                "arms warrior missing {:?}",
                f
            );
        }
        // Arms must NOT have protection-specific flags.
        assert!(!arms.get(BotStateKind::Combat).contains(F::PROTECTION));
        assert!(!arms.get(BotStateKind::Combat).contains(F::TANK_ASSIST));
    }

    /// Spot-check priest specs — all three share the same generic tail,
    /// but each has its own spec flag and off-spec crossover.
    #[test]
    fn priest_kit_strategies_match_pb2() {
        let disc = kit_strategies(PlayerClass::Priest, PlayerSpec::PriestDiscipline);
        let disc_c = disc.get(BotStateKind::Combat);
        assert!(disc_c.contains(F::DISCIPLINE));
        assert!(disc_c.contains(F::OFFHEAL));
        assert!(disc_c.contains(F::DPS_ASSIST | F::FLEE | F::CURE | F::RANGED));

        let holy = kit_strategies(PlayerClass::Priest, PlayerSpec::PriestHoly);
        assert!(holy.get(BotStateKind::Combat).contains(F::HOLY));
        assert!(holy.get(BotStateKind::Combat).contains(F::OFFDPS));

        let shadow = kit_strategies(PlayerClass::Priest, PlayerSpec::PriestShadow);
        assert!(shadow.get(BotStateKind::Combat).contains(F::SHADOW));
        assert!(shadow.get(BotStateKind::Combat).contains(F::OFFHEAL));
    }

    #[test]
    fn hunter_kit_strategies_match_pb2() {
        let surv = kit_strategies(PlayerClass::Hunter, PlayerSpec::HunterSurvival);
        let c = surv.get(BotStateKind::Combat);
        for f in [
            F::SURVIVAL,
            F::DPS_ASSIST,
            F::RANGED,
            F::CC,
            F::AOE,
            F::BUFF,
            F::BOOST,
            F::ASPECT,
            F::STING,
            F::PET,
        ] {
            assert!(c.contains(f), "survival hunter missing {:?}", f);
        }
    }

    #[test]
    fn rogue_kit_has_stealth_poisons_and_behind() {
        let sub = kit_strategies(PlayerClass::Rogue, PlayerSpec::RogueSubtlety);
        let c = sub.get(BotStateKind::Combat);
        assert!(c.contains(F::SUBTLETY));
        assert!(c.contains(F::STEALTH));
        assert!(c.contains(F::POISONS));
        assert!(c.contains(F::BEHIND));
        assert!(c.contains(F::CLOSE));
    }

    #[test]
    fn deathknight_kit_has_dksquest_and_spec_aoe() {
        let frost = kit_strategies(PlayerClass::DeathKnight, PlayerSpec::DeathKnightFrost);
        let c = frost.get(BotStateKind::Combat);
        assert!(c.contains(F::FROST));
        assert!(c.contains(F::FROST_AOE));
        assert!(c.contains(F::DKSQUEST));

        let unh = kit_strategies(PlayerClass::DeathKnight, PlayerSpec::DeathKnightUnholy);
        assert!(unh.get(BotStateKind::Combat).contains(F::UNHOLY));
        assert!(unh.get(BotStateKind::Combat).contains(F::UNHOLY_AOE));
    }
}
