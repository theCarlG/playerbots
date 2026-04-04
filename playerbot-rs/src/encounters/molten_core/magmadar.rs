/// Magmadar encounter FSM — Molten Core.
///
/// Single-phase fight.  Key mechanics:
///
/// 1. **Lava Bomb** (spell 19411): Ground AoE marker — bot must move out
///    immediately.  The bomb lands at a player's location.
///
/// 2. **Panic** (Fear, aura 19408): Magmadar fears the tank periodically.
///    Off-tanks / taunters need to pick up quickly.
///
/// 3. **Flame Buffet** (aura 19634): Stacking fire debuff on melee range targets.
///    Ranged and healers must stay > 30 yards from Magmadar to avoid it.
///
/// Bot positioning:
///   - Melee DPS and tank: stay close.
///   - Ranged and healers: maintain > 30 yards from Magmadar.

use super::super::{EncounterEvent, EncounterFsm};

pub const SPELL_LAVA_BOMB:    u32 = 19411;
pub const AURA_PANIC:         u32 = 19408; // fear
pub const AURA_FLAME_BUFFET:  u32 = 19634;

/// Minimum range for ranged/heals to avoid Flame Buffet.
pub const MAGMADAR_RANGED_MIN_RANGE: f32 = 30.0;

pub struct MagmadarFsm {
    active: bool,
    done:   bool,
}

impl MagmadarFsm {
    pub fn new() -> Self {
        Self { active: false, done: false }
    }
}

impl Default for MagmadarFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for MagmadarFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted          => self.active = true,
            EncounterEvent::UnitDied { victim: _ } => self.done = true,
            EncounterEvent::GroupWipe              => { self.active = false; }
            _ => {}
        }
    }

    fn phase_id(&self) -> u32  { if self.active { 1 } else { 0 } }
    fn is_active(&self) -> bool { self.active }
    fn is_done(&self)   -> bool { self.done }
    fn boss_entry(&self) -> u32 { super::ENTRY_MAGMADAR }
}
