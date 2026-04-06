/// Razorgore the Untamed encounter — Blackwing Lair.
///
/// Two-phase fight:
///   **Phase 1 (Egg Destroy):** One player mind-controls Razorgore via the
///     Orb of Domination and uses him to destroy all 30 eggs. The rest of
///     the raid kites adds (Dragonkin, Legionnaires, Mages). Adds spawn
///     in waves from the four corners. Tanks pick up adds, ranged AoE/kite.
///   **Phase 2 (Burn):** Once all eggs are destroyed, Razorgore breaks free
///     and becomes attackable. Standard tank-and-spank with:
///     - **War Stomp** (25188): PBAOE stun — melee spreads
///     - **Fireball Volley** (22425): unavoidable raid damage
///     - **Conflagration** (23023): targeted fire DoT
///
/// The MC controller is human — bots focus on add management in P1 and
/// DPS in P2.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, *};
use crate::ffi::SpellId;
use crate::{Sel, Seq};

pub const SPELL_WAR_STOMP: SpellId = SpellId(25188);
pub const SPELL_CONFLAGRATION: SpellId = SpellId(23023);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RazorgorePhase {
    #[default]
    Idle,
    /// Phase 1: add management while eggs are destroyed.
    EggDestroy,
    /// Phase 2: Razorgore attackable after eggs gone.
    Burn,
}

#[derive(Clone, PartialEq, Default)]
pub struct RazorgoreFsm {
    pub phase: RazorgorePhase,
    done: bool,
}

impl EncounterFsm for RazorgoreFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, _time: u64) {
        if self.done {
            return;
        }
        match event {
            EncounterEvent::CombatStarted => self.phase = RazorgorePhase::EggDestroy,
            EncounterEvent::UnitDied { .. } if self.phase != RazorgorePhase::Idle => {
                self.done = true;
            }
            EncounterEvent::GroupWipe => {
                self.phase = RazorgorePhase::Idle;
            }
            // Transition to burn phase when boss becomes attackable.
            // Heuristic: if boss HP starts dropping, eggs are done.
            EncounterEvent::None if self.phase == RazorgorePhase::EggDestroy => {
                if boss_hp_pct < 0.99 && boss_hp_pct > 0.0 {
                    self.phase = RazorgorePhase::Burn;
                }
            }
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            RazorgorePhase::Idle => 0,
            RazorgorePhase::EggDestroy => 1,
            RazorgorePhase::Burn => 2,
        }
    }

    fn is_active(&self) -> bool {
        self.phase != RazorgorePhase::Idle
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        super::ENTRY_RAZORGORE
    }

    fn phase_bt(&self) -> Option<Bt> {
        match self.phase {
            RazorgorePhase::Idle => None,
            // Phase 1: tanks pick up adds, ranged stay at range for kiting.
            RazorgorePhase::EggDestroy => Some(Sel!(
                Seq!(IsRanged, MaintainRange(25.0)),
            )),
            // Phase 2: standard burn. Melee behind to avoid War Stomp cleave.
            RazorgorePhase::Burn => Some(Sel!(
                Seq!(IsMeleeDps, MoveBehind(5.0)),
                Seq!(IsRanged, MaintainRange(20.0)),
            )),
        }
    }
}
