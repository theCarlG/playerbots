/// Bot initialization — builds the root behavior tree from (class, spec).
use crate::{
    bot::settings::{BehaviorMode, BotStateKind, StrategyFlags, StrategySet},
    bot::state::{BotState, PlayerClass, PlayerSpec},
    classes::{self, ClassKit},
    combat::reactive,
    engine::bt::Bt,
    ffi::{BotRole, interface::BotInterface},
    noncombat::GroupBuff,
    world,
};
use crate::{Seq, Sel};

/// Build a `BotState` from its handle, interface, class, and spec.
pub fn create_bot(
    handle: u64,
    interface: Box<dyn BotInterface>,
    class: PlayerClass,
    spec: PlayerSpec,
) -> Box<BotState> {
    let role = default_role_for_spec(&spec);
    let root_tree = build_root_tree(class, spec);
    let mut state = BotState::new(
        handle, interface, class, spec, role, root_tree,
    );
    // Layer PB2's per-class default strategies on top of the global
    // `PlayerbotAIConfig.cpp` baseline (empty combat/react/dead slots +
    // `+return,+delayed roll` in nonCombat). This matches PB2's
    // `AiFactory::Add*Strategies` runtime composition — see `pb2_kit_strategies`
    // for the per-class breakdown ported from `AiFactory.cpp`.
    state.settings.strategies = pb2_kit_strategies(class, spec);
    Box::new(state)
}

/// PB2 `AiFactory::AddDefaultCombatStrategies` + `AddDefaultNonCombatStrategies`
/// + per-class `AddClassDefaultCombatStrategies` composition.
///
/// Returns a `StrategySet` with the global baseline from
/// `PlayerbotAIConfig.cpp` plus every strategy name the PB2 kit for
/// `(class, spec)` turns on by default. This is the data side of
/// PARITY_PLAN §3.2.
///
/// **Scope note (Step 6):** populating these flags establishes the
/// correct per-bot default set for chat-filter and query responses. The
/// BT consumers that actually gate rotations on individual strategy
/// flags land in Step 11+. Until then, rotations continue to use
/// `CombatOrder` bits (see overlap note on `StrategyFlags`).
pub fn pb2_kit_strategies(class: PlayerClass, spec: PlayerSpec) -> StrategySet {
    use BotStateKind::{Combat, NonCombat};
    use PlayerClass::{
        DeathKnight, Druid, Hunter, Mage, Paladin, Priest, Rogue, Shaman, Warlock, Warrior,
    };
    use PlayerSpec::{
        DeathKnightBlood, DeathKnightFrost, DeathKnightUnholy, DruidBalance, DruidFeral,
        DruidRestoration, HunterBeastMastery, HunterMarksmanship, HunterSurvival, MageArcane,
        MageFire, MageFrost, PaladinHoly, PaladinProtection, PaladinRetribution, PriestDiscipline,
        PriestHoly, PriestShadow, RogueAssassination, RogueCombat, RogueSubtlety, ShamanElemental,
        ShamanEnhancement, ShamanRestoration, WarlockAffliction, WarlockDemonology,
        WarlockDestruction, WarriorArms, WarriorFury, WarriorProtection,
    };
    use StrategyFlags as F;

    // Start from the global PB2 baseline (empty Combat/Reaction/Dead +
    // NonCombat = RETURN | DELAYED_ROLL).
    let mut set = StrategySet::pb2_defaults();

    // ── All-bot combat base — PB2 `AiFactory::AddDefaultCombatStrategies`.
    //    Adds: mount, avoid mobs, racials, default, duel, pvp (non-BG).
    let combat_base = F::MOUNT | F::AVOID_MOBS | F::RACIALS | F::DEFAULT | F::DUEL | F::PVP;
    *set.get_mut(Combat) = set.get(Combat) | combat_base;

    // ── All-bot non-combat base — PB2 `AddDefaultNonCombatStrategies`.
    //    Adds: avoid mobs, wbuff. (`ai chat` only when `llmEnabled == 2`
    //    at runtime; skip until the LLM subsystem lands.)
    let noncombat_base = F::AVOID_MOBS | F::WBUFF;
    *set.get_mut(NonCombat) = set.get(NonCombat) | noncombat_base;

    // ── Per-class/spec combat additions — PB2 `AiFactory.cpp` class branches.
    //    The strings here match `AiFactory.cpp` verbatim (see PARITY_PLAN §3.2).
    //    `enableOffSpecStrategies` is assumed true (PB2 default), so `offheal`/
    //    `offdps` are added unconditionally.
    let class_add: StrategyFlags = match (class, spec) {
        // Warrior (tab-based in PB2 AiFactory.cpp).
        (Warrior, WarriorProtection) => {
            F::PROTECTION
                | F::TANK_ASSIST
                | F::PULL
                | F::PULL_BACK
                | F::CLOSE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
        }
        (Warrior, WarriorArms) => {
            F::ARMS | F::DPS_ASSIST | F::BEHIND | F::AOE | F::CC | F::BUFF | F::BOOST
        }
        (Warrior, WarriorFury) => {
            F::FURY | F::DPS_ASSIST | F::BEHIND | F::AOE | F::CC | F::BUFF | F::BOOST
        }

        // Priest (all specs share dps assist/flee/cure/ranged/cc/buff/aoe/boost).
        (Priest, PriestDiscipline) => {
            F::DISCIPLINE
                | F::OFFHEAL
                | F::DPS_ASSIST
                | F::FLEE
                | F::CURE
                | F::RANGED
                | F::CC
                | F::BUFF
                | F::AOE
                | F::BOOST
        }
        (Priest, PriestHoly) => {
            F::HOLY
                | F::OFFDPS
                | F::DPS_ASSIST
                | F::FLEE
                | F::CURE
                | F::RANGED
                | F::CC
                | F::BUFF
                | F::AOE
                | F::BOOST
        }
        (Priest, PriestShadow) => {
            F::SHADOW
                | F::OFFHEAL
                | F::DPS_ASSIST
                | F::FLEE
                | F::CURE
                | F::RANGED
                | F::CC
                | F::BUFF
                | F::AOE
                | F::BOOST
        }

        // Mage (all specs share dps assist/flee/cure/ranged/cc/buff/aoe/boost).
        (Mage, spec_m) => {
            let spec_flag = match spec_m {
                MageArcane => F::ARCANE,
                MageFire => F::FIRE,
                MageFrost => F::FROST,
                _ => F::NONE,
            };
            spec_flag
                | F::DPS_ASSIST
                | F::FLEE
                | F::CURE
                | F::RANGED
                | F::CC
                | F::BUFF
                | F::AOE
                | F::BOOST
        }

        // Warlock (all share dps assist/flee/ranged/cc/pet/aoe/buff/boost/curse).
        (Warlock, spec_w) => {
            let spec_flag = match spec_w {
                WarlockAffliction => F::AFFLICTION,
                WarlockDemonology => F::DEMONOLOGY,
                WarlockDestruction => F::DESTRUCTION,
                _ => F::NONE,
            };
            spec_flag
                | F::DPS_ASSIST
                | F::FLEE
                | F::RANGED
                | F::CC
                | F::PET
                | F::AOE
                | F::BUFF
                | F::BOOST
                | F::CURSE
        }

        // Paladin (tab-based). All specs add cure/aoe/cc/buff/boost/aura/blessing.
        (Paladin, PaladinProtection) => {
            F::PROTECTION
                | F::TANK_ASSIST
                | F::PULL
                | F::PULL_BACK
                | F::CLOSE
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
                | F::AURA
                | F::BLESSING
        }
        (Paladin, PaladinHoly) => {
            F::HOLY
                | F::OFFDPS
                | F::DPS_ASSIST
                | F::FLEE
                | F::RANGED
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
                | F::AURA
                | F::BLESSING
        }
        (Paladin, PaladinRetribution) => {
            F::RETRIBUTION
                | F::OFFHEAL
                | F::DPS_ASSIST
                | F::CLOSE
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
                | F::AURA
                | F::BLESSING
        }

        // Shaman. All specs add dps assist/cure/totems/buff/boost.
        (Shaman, ShamanElemental) => {
            F::ELEMENTAL
                | F::OFFHEAL
                | F::AOE
                | F::CC
                | F::FLEE
                | F::RANGED
                | F::DPS_ASSIST
                | F::CURE
                | F::TOTEMS
                | F::BUFF
                | F::BOOST
        }
        (Shaman, ShamanEnhancement) => {
            F::ENHANCEMENT
                | F::OFFHEAL
                | F::AOE
                | F::CC
                | F::CLOSE
                | F::DPS_ASSIST
                | F::CURE
                | F::TOTEMS
                | F::BUFF
                | F::BOOST
        }
        (Shaman, ShamanRestoration) => {
            F::RESTORATION
                | F::OFFDPS
                | F::FLEE
                | F::RANGED
                | F::DPS_ASSIST
                | F::CURE
                | F::TOTEMS
                | F::BUFF
                | F::BOOST
        }

        // Druid. All specs add cure/aoe/cc/buff/boost.
        (Druid, DruidBalance) => {
            F::BALANCE
                | F::OFFHEAL
                | F::DPS_ASSIST
                | F::FLEE
                | F::RANGED
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
        }
        (Druid, DruidFeral) => {
            // PB2 has separate "tank feral" and "dps feral" branches. The
            // Rust `PlayerSpec::DruidFeral` collapses them; default to the
            // dps-feral kit (the more common use), matching the Rust bot's
            // default role resolution. Tank-feral kits will be set once a
            // `DruidFeralTank` spec variant or `co +tank feral` override
            // lands.
            F::DPS_FERAL
                | F::OFFHEAL
                | F::DPS_ASSIST
                | F::CLOSE
                | F::BEHIND
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
        }
        (Druid, DruidRestoration) => {
            F::RESTORATION
                | F::OFFDPS
                | F::DPS_ASSIST
                | F::FLEE
                | F::RANGED
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
        }

        // Hunter. All specs add dps assist/ranged/cc/aoe/buff/boost/aspect/sting/pet.
        (Hunter, spec_h) => {
            let spec_flag = match spec_h {
                HunterBeastMastery => F::BEAST_MASTERY,
                HunterMarksmanship => F::MARKSMANSHIP,
                HunterSurvival => F::SURVIVAL,
                _ => F::NONE,
            };
            spec_flag
                | F::DPS_ASSIST
                | F::RANGED
                | F::CC
                | F::AOE
                | F::BUFF
                | F::BOOST
                | F::ASPECT
                | F::STING
                | F::PET
        }

        // Rogue. All specs add dps assist/aoe/close/cc/behind/stealth/poisons/buff/boost.
        (Rogue, spec_r) => {
            let spec_flag = match spec_r {
                RogueAssassination => F::ASSASSINATION,
                RogueCombat => F::ROGUE_COMBAT,
                RogueSubtlety => F::SUBTLETY,
                _ => F::NONE,
            };
            spec_flag
                | F::DPS_ASSIST
                | F::AOE
                | F::CLOSE
                | F::CC
                | F::BEHIND
                | F::STEALTH
                | F::POISONS
                | F::BUFF
                | F::BOOST
        }

        // Death Knight. All specs add dksquest/dps assist/flee/close/cc.
        (DeathKnight, DeathKnightBlood) => {
            F::BLOOD
                | F::TANK_ASSIST
                | F::PULL
                | F::PULL_BACK
                | F::DKSQUEST
                | F::DPS_ASSIST
                | F::FLEE
                | F::CLOSE
                | F::CC
        }
        (DeathKnight, DeathKnightFrost) => {
            F::FROST
                | F::FROST_AOE
                | F::DPS_ASSIST
                | F::DKSQUEST
                | F::FLEE
                | F::CLOSE
                | F::CC
        }
        (DeathKnight, DeathKnightUnholy) => {
            F::UNHOLY
                | F::UNHOLY_AOE
                | F::DPS_ASSIST
                | F::DKSQUEST
                | F::FLEE
                | F::CLOSE
                | F::CC
        }

        // Defensive fall-through: class/spec combinations that PB2 does
        // not kit (e.g. a mage spec passed with class=Warrior). Keep the
        // baseline empty rather than panicking so fuzz/test inputs with
        // mismatched (class, spec) tuples still produce a valid state.
        _ => F::NONE,
    };
    *set.get_mut(Combat) = set.get(Combat) | class_add;

    set
}

fn default_role_for_spec(spec: &PlayerSpec) -> BotRole {
    use PlayerSpec::{WarriorProtection, PaladinProtection, DruidFeral, PriestHoly, PriestDiscipline, PaladinHoly, ShamanRestoration, DruidRestoration};
    match spec {
        WarriorProtection | PaladinProtection | DruidFeral => BotRole::TANK,
        PriestHoly | PriestDiscipline | PaladinHoly | ShamanRestoration | DruidRestoration => {
            BotRole::HEAL
        }
        _ => BotRole::DPS,
    }
}

/// Look up the class rotation tree and buff list for this (class, spec).
/// Each class owns its own dispatch; this function is a flat 10-arm switch.
fn class_kit(class: PlayerClass, spec: PlayerSpec) -> ClassKit {
    use PlayerClass::{Warrior, Paladin, Priest, Druid, Hunter, Mage, Rogue, Shaman, Warlock, DeathKnight};
    match class {
        Warrior => classes::warrior::kit(spec),
        Paladin => classes::paladin::kit(spec),
        Priest => classes::priest::kit(spec),
        Druid => classes::druid::kit(spec),
        Hunter => classes::hunter::kit(spec),
        Mage => classes::mage::kit(spec),
        Rogue => classes::rogue::kit(spec),
        Shaman => classes::shaman::kit(spec),
        Warlock => classes::warlock::kit(spec),
        DeathKnight => classes::deathknight::kit(spec),
    }
}

/// Build the complete root behavior tree for a given class/spec.
///
/// Root structure (priority order):
///   1. Death handling (corpse run, accept rez)
///   2. Passive mode — do nothing
///   3. Eat/drink — recover HP/mana
///   4. Encounter override — boss mechanics
///   5. Combat wrapper (reactive + class rotation)
///   6. Mode-specific out-of-combat behavior
///   7. Maintenance (buff, loot, pet, mount, vendor, repair)
fn build_root_tree(class: PlayerClass, spec: PlayerSpec) -> Bt {
    use Bt::{ModeIs, InCombat, Consumables, EncounterOverride, ShouldEngage};

    let ClassKit {
        tree: combat_tree,
        buffs,
    } = class_kit(class, spec);

    Sel!(
        // 1. Death handling.
        world::death::death_subtree(),
        // 2. Passive mode — do nothing.
        ModeIs(BehaviorMode::Passive),
        // 3. Eat/drink — out of combat only.
        Seq!(InCombat.not(), Consumables),
        // 4. Encounter override.
        EncounterOverride,
        // 5. In combat → reactive + rotation.
        Seq!(
            Sel!(InCombat, ShouldEngage),
            combat_wrapper(combat_tree),
        ),
        // 6. Out-of-combat mode dispatch.
        mode_dispatch(),
        // 7. Maintenance.
        maintenance_subtree(buffs),
    )
}

/// Wrap a class rotation in the shared reactive subtrees (flee, interrupt,
/// dispel, threat, targeting) that apply to every class.
fn combat_wrapper(class_rotation: Bt) -> Bt {
    use Bt::MaintainConfiguredCurse;
    Sel!(
        reactive::flee_subtree(),
        reactive::interrupt_subtree(),
        reactive::dispel_subtree(),
        reactive::resurrect_subtree(),
        reactive::threat_subtree(),
        reactive::targeting_subtree(),
        // Warlock curse upkeep on the current target (no-op for other
        // classes — self-filters via `ClassPrefs::as_warlock`).
        Bt::throttle(2_000, MaintainConfiguredCurse),
        class_rotation,
    )
}

/// Mode dispatch — each behavior mode gets its own subtree.
fn mode_dispatch() -> Bt {
    use Bt::{ModeIs, Follow, StrategyEnabled};
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
                world::rpg::rpg_subtree(),
                world::grind::grind_subtree(),
            ),
        ),
        Seq!(
            ModeIs(BehaviorMode::Stay),
            world::stay::stay_subtree(),
        ),
        Seq!(
            ModeIs(BehaviorMode::Grind),
            world::grind::grind_subtree(),
        ),
        Seq!(
            ModeIs(BehaviorMode::Quest),
            world::quest::quest_subtree(),
        ),
        Seq!(
            ModeIs(BehaviorMode::Guard),
            world::guard::guard_subtree(),
        ),
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
        Seq!(
            InCombat.not(),
            Bt::throttle(30_000, ApplyShamanImbues),
        ),
        world::pet::pet_subtree(),
        world::loot::loot_subtree(),
        world::gather::gather_subtree(),
        world::mount::mount_subtree(),
        world::vendor::vendor_subtree(),
        world::repair::repair_subtree(),
        // Follow as absolute fallback.
        Bt::throttle(2_000, Follow),
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
            let set = pb2_kit_strategies(*c, *s);
            assert!(
                set.has(BotStateKind::Combat, base),
                "{:?}/{:?} missing all-bot combat base",
                c,
                s
            );
            // Non-combat base: avoid mobs + wbuff, plus PB2 defaults
            // (return + delayed roll).
            let nc_base =
                F::AVOID_MOBS | F::WBUFF | F::RETURN | F::DELAYED_ROLL;
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
        let prot = pb2_kit_strategies(PlayerClass::Warrior, PlayerSpec::WarriorProtection);
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

        let arms = pb2_kit_strategies(PlayerClass::Warrior, PlayerSpec::WarriorArms);
        for f in [F::ARMS, F::DPS_ASSIST, F::BEHIND, F::AOE, F::CC, F::BUFF, F::BOOST] {
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
        let disc = pb2_kit_strategies(PlayerClass::Priest, PlayerSpec::PriestDiscipline);
        let disc_c = disc.get(BotStateKind::Combat);
        assert!(disc_c.contains(F::DISCIPLINE));
        assert!(disc_c.contains(F::OFFHEAL));
        assert!(disc_c.contains(F::DPS_ASSIST | F::FLEE | F::CURE | F::RANGED));

        let holy = pb2_kit_strategies(PlayerClass::Priest, PlayerSpec::PriestHoly);
        assert!(holy.get(BotStateKind::Combat).contains(F::HOLY));
        assert!(holy.get(BotStateKind::Combat).contains(F::OFFDPS));

        let shadow = pb2_kit_strategies(PlayerClass::Priest, PlayerSpec::PriestShadow);
        assert!(shadow.get(BotStateKind::Combat).contains(F::SHADOW));
        assert!(shadow.get(BotStateKind::Combat).contains(F::OFFHEAL));
    }

    #[test]
    fn hunter_kit_strategies_match_pb2() {
        let surv = pb2_kit_strategies(PlayerClass::Hunter, PlayerSpec::HunterSurvival);
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
        let sub = pb2_kit_strategies(PlayerClass::Rogue, PlayerSpec::RogueSubtlety);
        let c = sub.get(BotStateKind::Combat);
        assert!(c.contains(F::SUBTLETY));
        assert!(c.contains(F::STEALTH));
        assert!(c.contains(F::POISONS));
        assert!(c.contains(F::BEHIND));
        assert!(c.contains(F::CLOSE));
    }

    #[test]
    fn deathknight_kit_has_dksquest_and_spec_aoe() {
        let frost = pb2_kit_strategies(PlayerClass::DeathKnight, PlayerSpec::DeathKnightFrost);
        let c = frost.get(BotStateKind::Combat);
        assert!(c.contains(F::FROST));
        assert!(c.contains(F::FROST_AOE));
        assert!(c.contains(F::DKSQUEST));

        let unh = pb2_kit_strategies(PlayerClass::DeathKnight, PlayerSpec::DeathKnightUnholy);
        assert!(unh.get(BotStateKind::Combat).contains(F::UNHOLY));
        assert!(unh.get(BotStateKind::Combat).contains(F::UNHOLY_AOE));
    }
}
