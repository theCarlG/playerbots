/// Baron Geddon encounter FSM — Molten Core.
///
/// Single-phase fight but with two critical bot mechanics:
///
/// 1. **Living Bomb** (aura 20475): The bot with this debuff MUST immediately
///    run far from the raid — it explodes after ~5s dealing massive AoE damage.
///    Bot should move to the outer edge of the room and stop attacking.
///
/// 2. **Inferno** (aura 19695): Geddon channels an AoE ring around himself.
///    ALL bots must flee the ring immediately (priority over everything else).
///
/// Bot behavior:
///   - Living Bomb holder: flee to edge of room (> 30 yards from raid center).
///   - Inferno active: flee safe position.
///   - Normal: DPS / heal from max range.

use super::super::{EncounterEvent, EncounterFsm};

/// Aura spell IDs
pub const AURA_LIVING_BOMB: u32 = 20475;
pub const AURA_INFERNO:     u32 = 19695;

pub struct BaronGeddonFsm {
    active: bool,
    done:   bool,
}

impl BaronGeddonFsm {
    pub fn new() -> Self {
        Self { active: false, done: false }
    }
}

impl Default for BaronGeddonFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for BaronGeddonFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted               => self.active = true,
            EncounterEvent::UnitDied { victim: _ }      => self.done = true,
            EncounterEvent::GroupWipe                   => { self.active = false; }
            _ => {}
        }
    }

    fn phase_id(&self) -> u32  { if self.active { 1 } else { 0 } }
    fn is_active(&self) -> bool { self.active }
    fn is_done(&self)   -> bool { self.done }
    fn boss_entry(&self) -> u32 { super::ENTRY_BARON_GEDDON }
}
