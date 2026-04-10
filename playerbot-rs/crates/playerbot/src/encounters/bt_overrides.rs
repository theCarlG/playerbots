/// Encounter BT override — bridges the encounter FSM's per-phase BT into the
/// root behavior tree.
///
/// Now handled by the `Bt::EncounterOverride` variant in `engine::bt`.
/// This module is kept for backwards compatibility and tests.

#[cfg(test)]
mod tests {
    use crate::bot::state::PlayerClass;
    use crate::encounters::{EncounterEvent, EncounterFsm};
    use crate::engine::bt::Bt;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, make_encounter_ctx};
    use cmangos::MockWorld;
    use cmangos::BotRole;

    /// Mock encounter that returns no BT.
    struct NoOpEncounter;
    impl EncounterFsm for NoOpEncounter {
        fn update(&mut self, _: &EncounterEvent, _: f32, _: u64) {}
        fn phase_id(&self) -> u32 {
            1
        }
        fn is_active(&self) -> bool {
            true
        }
        fn is_done(&self) -> bool {
            false
        }
        fn boss_entry(&self) -> u32 {
            99999
        }
    }

    #[test]
    fn no_encounter_returns_failure() {
        let tree = Bt::EncounterOverride;
        let mut owned = TestCtxOwned::new();
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn unknown_boss_returns_failure() {
        let tree = Bt::EncounterOverride;
        let enc = NoOpEncounter;
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &enc, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Failure);
    }
}
