/// Broodlord Lashlayer — Blackwing Lair.
///
/// Single-phase fight. Key mechanics:
///   - **Knock Back** (18670): cone knockback, pushes melee off the boss.
///     Tank must wall-hug so the knockback doesn't launch him off the edge.
///   - **Mortal Strike** (24573): tank takes a healing-reduction debuff.
///     A reactive tank-swap is handled by the targeting layer — not here.
///   - **Blast Wave** (23331): short-range fire nova on cast — melee back off briefly.
///
/// Strategy: melee hug the boss from the sides/back, ranged stay at max
/// range to minimise knockback exposure.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, Sel, Seq, IsRanged, MaintainRange, IsMeleeDps};
use crate::ffi::SpellId;

pub const SPELL_KNOCK_BACK: SpellId = SpellId(18670);
pub const SPELL_MORTAL_STRIKE: SpellId = SpellId(24573);
pub const SPELL_BLAST_WAVE: SpellId = SpellId(23331);

#[derive(Clone, Debug)]
pub struct BroodlordFsm {
    active: bool,
    done: bool,
    bt: Bt,
}

impl PartialEq for BroodlordFsm {
    fn eq(&self, other: &Self) -> bool {
        self.active == other.active && self.done == other.done
    }
}

impl BroodlordFsm {
    pub fn new() -> Self {
        Self {
            active: false,
            done: false,
            bt: Self::build_bt(),
        }
    }

    fn build_bt() -> Bt {
        // Ranged stay at max range (30y) to avoid the knockback cone
        // and Blast Wave; melee hug close to stay in the safe arc.
        Sel(vec![
            Seq(vec![IsRanged, MaintainRange(30.0)]),
            Seq(vec![IsMeleeDps, MaintainRange(5.0)]),
        ])
    }
}

impl Default for BroodlordFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for BroodlordFsm {
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
        super::ENTRY_BROODLORD
    }
    fn phase_bt(&self) -> Option<&Bt> {
        if self.active { Some(&self.bt) } else { None }
    }
}
