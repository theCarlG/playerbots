/// Bot initialization — builds the root behavior tree from (class, spec).
use crate::{
    bot::settings::{BehaviorMode, StrategyFlags},
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
    Box::new(BotState::new(
        handle, interface, class, spec, role, root_tree,
    ))
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
