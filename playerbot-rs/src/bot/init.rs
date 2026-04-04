/// Bot initialization — builds the root behavior tree from (class, spec).
use crate::{
    bot::state::{BotState, PlayerClass, PlayerSpec},
    engine::bt_nodes::{BtNode, cond, sel},
    ffi::{interface::BotInterface, BotRole},
    noncombat,
};

/// Build a BotState from its handle, interface, class, and spec.
pub fn create_bot(
    handle: u64,
    interface: Box<dyn BotInterface>,
    class: PlayerClass,
    spec: PlayerSpec,
) -> Box<BotState> {
    let role = default_role_for_spec(&spec);
    let root_tree = build_root_tree(class, spec);
    Box::new(BotState::new(handle, interface, class, spec, role, root_tree))
}

fn default_role_for_spec(spec: &PlayerSpec) -> BotRole {
    use PlayerSpec::*;
    match spec {
        WarriorProtection | PaladinProtection | DruidFeral => BotRole::TANK,
        PriestHoly | PriestDiscipline | PaladinHoly | ShamanRestoration
        | DruidRestoration => BotRole::HEAL,
        _ => BotRole::DPS,
    }
}

/// Build the complete root behavior tree for a given class/spec.
///
/// Root structure:
///   Selector [
///     noncombat_subtree   (eat/drink → buff → loot → follow)
///     combat_subtree      (class-specific rotation)
///   ]
fn build_root_tree(class: PlayerClass, spec: PlayerSpec) -> Box<dyn BtNode> {
    use crate::classes::*;
    use PlayerClass::*;
    use PlayerSpec::*;

    let (combat_tree, buffs) = match (class, spec) {
        // ── Warrior ───────────────────────────────────────────────────────
        (Warrior, WarriorArms)       => (warrior::arms::build_tree(),       noncombat::warrior_buffs()),
        (Warrior, WarriorFury)       => (warrior::fury::build_tree(),        noncombat::warrior_buffs()),
        (Warrior, WarriorProtection) => (warrior::protection::build_tree(),  noncombat::warrior_buffs()),

        // ── Paladin ───────────────────────────────────────────────────────
        (Paladin, PaladinRetribution) => (paladin::retribution::build_tree(), noncombat::paladin_retribution_buffs()),
        (Paladin, PaladinHoly)        => (paladin::holy::build_tree(),         noncombat::paladin_holy_buffs()),
        (Paladin, PaladinProtection)  => (paladin::protection::build_tree(),   noncombat::paladin_protection_buffs()),

        // ── Priest ────────────────────────────────────────────────────────
        (Priest, PriestHoly)       => (priest::holy::build_tree(),       noncombat::priest_buffs()),
        (Priest, PriestDiscipline) => (priest::discipline::build_tree(), noncombat::priest_buffs()),
        (Priest, PriestShadow)     => (priest::shadow::build_tree(),     noncombat::priest_buffs()),

        // ── Druid ─────────────────────────────────────────────────────────
        (Druid, DruidBalance)     => (druid::balance::build_tree(),     noncombat::druid_buffs()),
        (Druid, DruidFeral)       => (druid::feral::build_tree(),       noncombat::druid_buffs()),
        (Druid, DruidRestoration) => (druid::restoration::build_tree(), noncombat::druid_buffs()),

        // ── Hunter ────────────────────────────────────────────────────────
        (Hunter, HunterBeastMastery)  => (hunter::beast_mastery::build_tree(),  noncombat::no_buffs()),
        (Hunter, HunterMarksmanship)  => (hunter::marksmanship::build_tree(),   noncombat::no_buffs()),
        (Hunter, HunterSurvival)      => (hunter::survival::build_tree(),       noncombat::no_buffs()),

        // ── Mage ──────────────────────────────────────────────────────────
        (Mage, MageArcane) => (mage::arcane::build_tree(), noncombat::mage_buffs()),
        (Mage, MageFire)   => (mage::fire::build_tree(),   noncombat::mage_buffs()),
        (Mage, MageFrost)  => (mage::frost::build_tree(),  noncombat::mage_buffs()),

        // ── Rogue ─────────────────────────────────────────────────────────
        (Rogue, RogueAssassination) => (rogue::assassination::build_tree(), noncombat::no_buffs()),
        (Rogue, RogueCombat)        => (rogue::combat::build_tree(),        noncombat::no_buffs()),
        (Rogue, RogueSubtlety)      => (rogue::subtlety::build_tree(),      noncombat::no_buffs()),

        // ── Shaman ────────────────────────────────────────────────────────
        (Shaman, ShamanElemental)    => (shaman::elemental::build_tree(),    noncombat::shaman_elemental_buffs()),
        (Shaman, ShamanEnhancement)  => (shaman::enhancement::build_tree(),  noncombat::shaman_enhancement_buffs()),
        (Shaman, ShamanRestoration)  => (shaman::restoration::build_tree(),  noncombat::shaman_restoration_buffs()),

        // ── Warlock ───────────────────────────────────────────────────────
        (Warlock, WarlockAffliction)  => (warlock::affliction::build_tree(),  noncombat::warlock_buffs()),
        (Warlock, WarlockDemonology)  => (warlock::demonology::build_tree(),  noncombat::warlock_buffs()),
        (Warlock, WarlockDestruction) => (warlock::destruction::build_tree(), noncombat::warlock_buffs()),

        // ── Death Knight (WotLK only) ─────────────────────────────────────
        (DeathKnight, DeathKnightBlood)  => (deathknight::blood::build_tree(),  noncombat::no_buffs()),
        (DeathKnight, DeathKnightFrost)  => (deathknight::frost::build_tree(),  noncombat::no_buffs()),
        (DeathKnight, DeathKnightUnholy) => (deathknight::unholy::build_tree(), noncombat::no_buffs()),

        // Invalid class/spec combinations (should never happen in practice)
        _ => return sel(vec![cond(|_| false)]),
    };

    // Wrap the combat tree with non-combat behavior at the top level.
    sel(vec![
        noncombat::build_noncombat_subtree(buffs),
        combat_tree,
    ])
}
