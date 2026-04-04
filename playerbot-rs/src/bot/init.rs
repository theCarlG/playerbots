/// Bot initialization — builds the root behavior tree from (class, spec).
use crate::{
    bot::state::{BotRole, BotState, PlayerClass, PlayerSpec},
    engine::bt_nodes::{BtNode, cond, sel},
    ffi::interface::BotInterface,
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
fn build_root_tree(class: PlayerClass, spec: PlayerSpec) -> Box<dyn BtNode> {
    use crate::classes::*;
    use PlayerClass::*;
    use PlayerSpec::*;

    match (class, spec) {
        // ── Warrior ───────────────────────────────────────────────────────
        (Warrior, WarriorArms)       => warrior::arms::build_tree(),
        (Warrior, WarriorFury)       => warrior::fury::build_tree(),
        (Warrior, WarriorProtection) => warrior::protection::build_tree(),

        // ── Paladin ───────────────────────────────────────────────────────
        (Paladin, PaladinRetribution) => paladin::retribution::build_tree(),
        (Paladin, PaladinHoly)        => paladin::holy::build_tree(),
        (Paladin, PaladinProtection)  => paladin::protection::build_tree(),

        // ── Priest ────────────────────────────────────────────────────────
        (Priest, PriestHoly)       => priest::holy::build_tree(),
        (Priest, PriestDiscipline) => priest::discipline::build_tree(),
        (Priest, PriestShadow)     => priest::shadow::build_tree(),

        // ── Druid ─────────────────────────────────────────────────────────
        (Druid, DruidBalance)     => druid::balance::build_tree(),
        (Druid, DruidFeral)       => druid::feral::build_tree(),
        (Druid, DruidRestoration) => druid::restoration::build_tree(),

        // ── Hunter ────────────────────────────────────────────────────────
        (Hunter, HunterBeastMastery)  => hunter::beast_mastery::build_tree(),
        (Hunter, HunterMarksmanship)  => hunter::marksmanship::build_tree(),
        (Hunter, HunterSurvival)      => hunter::survival::build_tree(),

        // ── Mage ──────────────────────────────────────────────────────────
        (Mage, MageArcane) => mage::arcane::build_tree(),
        (Mage, MageFire)   => mage::fire::build_tree(),
        (Mage, MageFrost)  => mage::frost::build_tree(),

        // ── Rogue ─────────────────────────────────────────────────────────
        (Rogue, RogueAssassination) => rogue::assassination::build_tree(),
        (Rogue, RogueCombat)        => rogue::combat::build_tree(),
        (Rogue, RogueSubtlety)      => rogue::subtlety::build_tree(),

        // ── Shaman ────────────────────────────────────────────────────────
        (Shaman, ShamanElemental)    => shaman::elemental::build_tree(),
        (Shaman, ShamanEnhancement)  => shaman::enhancement::build_tree(),
        (Shaman, ShamanRestoration)  => shaman::restoration::build_tree(),

        // ── Warlock ───────────────────────────────────────────────────────
        (Warlock, WarlockAffliction)  => warlock::affliction::build_tree(),
        (Warlock, WarlockDemonology)  => warlock::demonology::build_tree(),
        (Warlock, WarlockDestruction) => warlock::destruction::build_tree(),

        // ── Death Knight (WotLK only) ─────────────────────────────────────
        (DeathKnight, DeathKnightBlood)  => deathknight::blood::build_tree(),
        (DeathKnight, DeathKnightFrost)  => deathknight::frost::build_tree(),
        (DeathKnight, DeathKnightUnholy) => deathknight::unholy::build_tree(),

        // Invalid class/spec combinations (should never happen in practice)
        _ => sel(vec![cond(|_| false)]),
    }
}
