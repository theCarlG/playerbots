/// Arcane Mage behavior tree (Classic / Vanilla).
///
/// While LEVELING, every mage plays as frost (per Icy Veins): Frostbolt is the
/// reliable single-target nuke at every level, with Frost Nova / Cone of Cold /
/// Blink for survival. Arcane has no viable leveling nuke of its own — the old
/// arcane tree opened with **Arcane Missiles**, a channeled spell that breaks on
/// any movement (the bot "cast Arcane Missiles, it failed but drained mana") and
/// was prioritised over Frostbolt. So the arcane spec delegates its combat tree
/// to the shared frost leveling rotation, which uses only baseline mage spells.
use crate::engine::bt::Bt;
use crate::engine::macro_fsm::ActiveFsm;

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => super::frost::combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}
