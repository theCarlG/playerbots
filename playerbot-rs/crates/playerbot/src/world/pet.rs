/// Pet management — summon, revive, feed (Hunter/Warlock).
use crate::engine::bt::Bt::{self, IsPetClass, HasPet, PetAlive, InCombat, RevivePet, SummonPet, IsClass, PetUnhappy, FeedPet};
use crate::{Sel, Seq};

pub fn pet_subtree() -> Bt {
    Seq!(
        IsPetClass,
        Sel!(
            // Revive a dead pet. Hunter Revive Pet is castable in combat (a
            // ~10s cast, but it beats waiting for the fight to end and losing
            // the pet's DPS/utility the whole time), so it is NOT gated on
            // being out of combat. Warlocks lose the pet entirely on death
            // (HasPet → false) and fall through to the summon branch below,
            // whose re-summon stays out-of-combat.
            Seq!(
                IsClass(crate::bot::state::PlayerClass::Hunter),
                HasPet,
                PetAlive.not(),
                RevivePet,
            ),
            // Summon pet if none.
            Seq!(HasPet.not(), InCombat.not(), SummonPet),
            // Feed unhappy pet (Hunter only).
            Seq!(
                IsClass(crate::bot::state::PlayerClass::Hunter),
                HasPet,
                PetAlive,
                PetUnhappy,
                InCombat.not(),
                Bt::throttle(30_000, FeedPet),
            ),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::state::PlayerClass;
    use crate::encounters::EncounterEvent;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, make_encounter_ctx};
    use cmangos::BotRole;
    use cmangos::MockEvent;
    use cmangos::MockWorld;

    /// Trivial always-active encounter so `make_encounter_ctx` is usable for a
    /// non-encounter subtree test.
    struct NoEnc;
    impl crate::encounters::EncounterFsm for NoEnc {
        fn update(&mut self, _e: &EncounterEvent, _hp: f32, _t: u64) {}
        fn phase_id(&self) -> u32 {
            0
        }
        fn is_active(&self) -> bool {
            false
        }
        fn is_done(&self) -> bool {
            false
        }
        fn boss_entry(&self) -> u32 {
            0
        }
    }

    #[test]
    fn hunter_revives_dead_pet_in_combat() {
        let tree = pet_subtree();
        let iface = MockWorld::new().with_dead_pet();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.in_combat = true; // mid-fight
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &NoEnc, PlayerClass::Hunter, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::RevivePet)),
            "hunter revives its pet without waiting for combat to end"
        );
    }

    #[test]
    fn warlock_does_not_revive_in_combat() {
        // A warlock loses its pet on death (has_pet → false) and must re-summon,
        // which stays out-of-combat: no revive, no summon while fighting.
        let tree = pet_subtree();
        let iface = MockWorld::new(); // no pet
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.in_combat = true;
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &NoEnc, PlayerClass::Warlock, BotRole::DPS);
        assert_eq!(tree.tick(&mut ctx), BtResult::Failure);
    }
}
