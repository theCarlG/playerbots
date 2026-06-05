/// Baron Geddon encounter — Molten Core.
///
/// Single-phase fight with two mechanics. The shared rule (how every bot uses
/// a defensive): **get out of the danger FIRST, then pop the cooldown** — a
/// personal immunity is never a substitute for moving, because:
///   - Ice Block / Divine Shield don't stop the Living Bomb explosion from
///     hitting the *rest* of the raid, and Ice Block roots the mage, so popping
///     it before clearing the raid traps the bot on top of everyone.
///   - So the bot relocates clear of the raid, and ONLY once clear does a mage
///     Ice Block / paladin Divine Shield to negate the detonation's self-damage
///     and knock-up; classes without an immunity just hold out of range.
///
/// 1. **Living Bomb** (aura 20475): the bombed bot detonates after ~8s, hitting
///    everyone nearby and knocking them up. The debuffed bot walks clear of the
///    group, then pops its immunity. Bots WITHOUT the debuff are unaffected —
///    `self_has` checks this bot only.
///
/// 2. **Inferno** (aura 19695 on Geddon): he channels an escalating `PBAoE`
///    ring. Everyone flees out of the radius (30yd) — fleeing *is* the defense
///    here, there's no immunity to pop afterward.
use super::super::{EncounterEvent, EncounterFsm};
use crate::bot::state::PlayerClass;
use crate::encounters::bt::Bt::{self, CastOnSelf, FleeToSafe, HoldPosition, IsClass};
use crate::engine::bt::BehaviorLeaf;
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const AURA_LIVING_BOMB: SpellId = SpellId(20475);
pub const AURA_INFERNO: SpellId = SpellId(19695);

const ICE_BLOCK: SpellId = SpellId(11958);
const DIVINE_SHIELD: SpellId = SpellId(642);

/// A groupmate this close means the bomb-carrier is still on top of the raid.
const LIVING_BOMB_SAFE_DIST: f32 = 12.0;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BaronGeddonFsm {
    active: bool,
    done: bool,
}

impl BaronGeddonFsm {
    fn build_bt() -> Bt {
        Sel!(Self::living_bomb(), Self::inferno())
    }

    /// Living Bomb: walk clear of the raid, THEN pop a personal immunity.
    /// `MOVE_CLEAR_OF_RAID` runs (Running) until the bot is clear, then yields
    /// (Failure) so the `Sel` drops through — a mage Ice Blocks, a paladin
    /// Divine Shields, anyone else just holds out of range until it detonates.
    fn living_bomb() -> Bt {
        Seq!(
            Bt::self_has(AURA_LIVING_BOMB),
            Sel!(
                Bt::Custom(MOVE_CLEAR_OF_RAID),
                Seq!(IsClass(PlayerClass::Mage), CastOnSelf(ICE_BLOCK)),
                Seq!(IsClass(PlayerClass::Paladin), CastOnSelf(DIVINE_SHIELD)),
                HoldPosition,
            ),
        )
    }

    /// Inferno: everyone flees out of the boss's escalating `PBAoE`.
    fn inferno() -> Bt {
        Seq!(Bt::target_has(AURA_INFERNO), FleeToSafe(30.0))
    }
}

/// Move the bomb-carrier clear of the raid, then yield. While any groupmate is
/// within `LIVING_BOMB_SAFE_DIST`, step directly away from the cluster's
/// centroid (Running). Once clear — or solo — return Failure so the caller can
/// pop a defensive in the bot's now-isolated position.
const MOVE_CLEAR_OF_RAID: BehaviorLeaf = BehaviorLeaf {
    label: "geddon_move_clear_of_raid",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        let me = ctx.snap.self_.pos;
        let mut cx = 0.0f32;
        let mut cy = 0.0f32;
        let mut n = 0u32;
        let mut nearest_sq = f32::MAX;
        for &h in ctx.snap.group_members[..ctx.snap.group_size as usize].iter() {
            if h == 0 || h == ctx.bot_handle {
                continue;
            }
            let Some(p) = ctx.interface.get_player_position(h) else {
                continue;
            };
            let d2 = (me.x - p.x).powi(2) + (me.y - p.y).powi(2);
            if d2 < nearest_sq {
                nearest_sq = d2;
            }
            cx += p.x;
            cy += p.y;
            n += 1;
        }
        // Solo, or already clear of every groupmate — let the defensive fire.
        if n == 0 || nearest_sq >= LIVING_BOMB_SAFE_DIST * LIVING_BOMB_SAFE_DIST {
            return BtResult::Failure;
        }
        cx /= n as f32;
        cy /= n as f32;
        // Step out along the centroid→bot direction, past the safe distance.
        let (mut dx, mut dy) = (me.x - cx, me.y - cy);
        let len = dx.hypot(dy);
        if len < 0.001 {
            // Standing on the centroid — pick an arbitrary axis to break out.
            dx = 1.0;
            dy = 0.0;
        } else {
            dx /= len;
            dy /= len;
        }
        let dest_x = cx + dx * (LIVING_BOMB_SAFE_DIST + 4.0);
        let dest_y = cy + dy * (LIVING_BOMB_SAFE_DIST + 4.0);
        if ctx.interface.move_to(dest_x, dest_y, me.z) {
            BtResult::Running
        } else {
            BtResult::Failure
        }
    },
    display_text: Some("Bomb! Clearing raid"),
};

impl EncounterFsm for BaronGeddonFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { victim: _ } => self.done = true,
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
        super::ENTRY_BARON_GEDDON
    }

    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Self::build_bt())
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

    const MATE: u64 = 777;

    #[test]
    fn living_bomb_carrier_clears_raid_before_defensive() {
        // A groupmate is right next to the bombed mage — it relocates out of
        // the raid (Running + MoveTo) and does NOT Ice Block yet.
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new()
            .with_aura(AURA_LIVING_BOMB)
            .with_player_position(MATE, 5.0, 0.0, 0.0); // 5y away — too close
        let mut owned = TestCtxOwned::new();
        owned.snap.group_members[0] = MATE;
        owned.snap.group_size = 1;
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert!(matches!(bt.tick(&mut ctx), BtResult::Running));
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::MoveTo { .. })),
            "bombed bot relocates before popping a defensive"
        );
    }

    #[test]
    fn living_bomb_clear_mage_ice_blocks() {
        // Once clear of the raid (no groupmate nearby), the mage pops Ice Block.
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_aura(AURA_LIVING_BOMB);
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface.events().iter().any(|e| matches!(
                e,
                MockEvent::CastSpell { spell, .. } if *spell == ICE_BLOCK
            )),
            "clear mage Ice Blocks"
        );
    }

    #[test]
    fn living_bomb_clear_warrior_holds() {
        // A class with no personal immunity just holds, clear of the raid,
        // until the bomb detonates.
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_aura(AURA_LIVING_BOMB);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn inferno_mage_flees_not_fire_wards() {
        // Inferno: even a mage flees the radius rather than standing in it
        // behind a Fire Ward.
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_aura(AURA_INFERNO).with_safe_pos();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 100; // Baron Geddon
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert!(matches!(bt.tick(&mut ctx), BtResult::Running));
    }

    #[test]
    fn no_mechanic_returns_failure() {
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn no_bt_when_idle() {
        assert!(
            BaronGeddonFsm::default()
                .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
