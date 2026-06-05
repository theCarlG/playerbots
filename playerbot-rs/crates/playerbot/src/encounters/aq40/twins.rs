/// The Twin Emperors (Vek'lor + Vek'nilash) — Temple of Ahn'Qiraj.
///
/// Vek'nilash is the melee twin; Vek'lor is the caster twin who casts a frontal
/// Arcane Burst that knocks back anyone meleeing him. So the role split is:
///   - **Melee DPS → Vek'nilash** (15275): safe to melee.
///   - **Ranged DPS → Vek'lor** (15276): hit him from range; don't melee him.
/// They also teleport-swap and heal each other when close — that tanking/spacing
/// is a raid-coordination job, not scripted here; this just keeps each role on
/// the right twin.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, IsMeleeDps, IsRanged};
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq)]
pub struct TwinsFsm {
    entry: u32,
    active: bool,
    done: bool,
}

impl TwinsFsm {
    pub fn new(entry: u32) -> Self {
        Self {
            entry,
            active: false,
            done: false,
        }
    }
}

impl EncounterFsm for TwinsFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { .. } => self.done = true,
            EncounterEvent::GroupWipe => self.active = false,
            _ => {}
        }
    }
    fn phase_id(&self) -> u32 {
        u32::from(self.active)
    }
    fn is_active(&self) -> bool {
        self.active
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        self.entry
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(
                Seq!(
                    IsMeleeDps,
                    Bt::FocusNearestEntry(super::ENTRY_EMPEROR_VEKNILASH)
                ),
                Seq!(IsRanged, Bt::FocusNearestEntry(super::ENTRY_EMPEROR_VEKLOR)),
            ))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::state::PlayerClass;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, make_encounter_ctx};
    use cmangos::BotRole;
    use cmangos::MockEvent;
    use cmangos::MockWorld;

    #[test]
    fn melee_takes_veknilash() {
        const VEKNILASH: u64 = 75;
        let mut fsm = TwinsFsm::new(super::super::ENTRY_EMPEROR_VEKNILASH);
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface =
            MockWorld::new().with_nearby_entry(VEKNILASH, super::super::ENTRY_EMPEROR_VEKNILASH);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == VEKNILASH)),
            "melee target the melee twin"
        );
    }

    #[test]
    fn ranged_takes_veklor() {
        const VEKLOR: u64 = 76;
        let mut fsm = TwinsFsm::new(super::super::ENTRY_EMPEROR_VEKLOR);
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_nearby_entry(VEKLOR, super::super::ENTRY_EMPEROR_VEKLOR);
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == VEKLOR)),
            "ranged target the caster twin"
        );
    }
}
