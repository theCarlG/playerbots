/// Grobbulus encounter FSM — Naxxramas, Construct Wing.
///
/// Single-phase fight but with a critical mechanic:
/// **Mutating Injection** (aura 28169): Applied to a random player.
/// The afflicted player MUST immediately run away from the raid (at least 20 yards).
/// After ~10s the injection explodes, dealing AoE damage.
/// If the explosion hits other raid members they gain Slime spray → wipe risk.
///
/// Additionally: Grobbulus slowly walks in a circle (bots must stay behind him).
/// Slime Spray (28158): frontal cone — tank must keep boss turned away.

use super::super::{EncounterEvent, EncounterFsm};
use crate::ffi::SpellId;

pub const AURA_MUTATING_INJECTION: SpellId = SpellId(28169);
pub const SPELL_SLIME_SPRAY:       SpellId = SpellId(28158);

pub struct GrobbolusFsm {
    active: bool,
    done:   bool,
}

impl GrobbolusFsm {
    pub fn new() -> Self {
        Self { active: false, done: false }
    }
}

impl Default for GrobbolusFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for GrobbolusFsm {
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
    fn boss_entry(&self) -> u32 { super::ENTRY_GROBBULUS }
}
