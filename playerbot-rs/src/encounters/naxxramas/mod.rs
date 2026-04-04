/// Naxxramas — 15 boss encounters across 5 wings.
///
/// Zone ID: 3456.  40-player raid (Vanilla) / 10 and 25-player (WotLK).
///
/// Wings:
///   Spider Wing:    Anub'Rekhan, Grand Widow Faerlina, Maexxna
///   Plague Wing:    Noth the Plaguebringer, Heigan the Unclean, Loatheb
///   Military Wing:  Instructor Razuvious, Gothik the Harvester, Four Horsemen
///   Construct Wing: Patchwerk, Grobbulus, Gluth, Thaddius
///   Frostwyrm Lair: Sapphiron, Kel'Thuzad

pub mod heigan;
pub mod thaddius;
pub mod kel_thuzad;
pub mod grobbulus;

pub use heigan::HeiganFsm;
pub use thaddius::ThaddiusFsm;
pub use kel_thuzad::KelThuzadFsm;
pub use grobbulus::GrobbolusFsm;

use super::{EncounterEvent, EncounterFsm, SimpleFsm};
use crate::engine::bt::Bt;

// ── NPC entry IDs ─────────────────────────────────────────────────────────

pub const ENTRY_ANUB_REKHAN:     u32 = 15956;
pub const ENTRY_FAERLINA:        u32 = 15953;
pub const ENTRY_MAEXXNA:         u32 = 15952;
pub const ENTRY_NOTH:            u32 = 15954;
pub const ENTRY_HEIGAN:          u32 = 15302;
pub const ENTRY_LOATHEB:         u32 = 16011;
pub const ENTRY_RAZUVIOUS:       u32 = 16061;
pub const ENTRY_GOTHIK:          u32 = 16060;
pub const ENTRY_THANE:           u32 = 16063;
pub const ENTRY_PATCHWERK:       u32 = 16028;
pub const ENTRY_GROBBULUS:       u32 = 15931;
pub const ENTRY_GLUTH:           u32 = 15932;
pub const ENTRY_THADDIUS:        u32 = 15928;
pub const ENTRY_SAPPHIRON:       u32 = 15989;
pub const ENTRY_KEL_THUZAD:      u32 = 15990;

/// Active boss as a sum type: specialized FSMs live as variant payloads,
/// simple fights are payload-less variants. A fresh FSM is constructed on
/// `from_entry`, so state resets cleanly between pulls.
#[derive(Clone, PartialEq)]
enum Boss {
    None,
    AnubRekhan, Faerlina, Maexxna,
    Noth, Heigan(HeiganFsm), Loatheb,
    Razuvious, Gothik, FourHorsemen,
    Patchwerk, Grobbulus(GrobbolusFsm), Gluth, Thaddius(ThaddiusFsm),
    Sapphiron, KelThuzad(KelThuzadFsm),
}

impl Boss {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        match self {
            Self::Heigan(fsm)    => fsm.update(event, boss_hp_pct, time_ms),
            Self::Thaddius(fsm)  => fsm.update(event, boss_hp_pct, time_ms),
            Self::KelThuzad(fsm) => fsm.update(event, boss_hp_pct, time_ms),
            Self::Grobbulus(fsm) => fsm.update(event, boss_hp_pct, time_ms),
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self {
            Self::Heigan(fsm)    => fsm.phase_id(),
            Self::Thaddius(fsm)  => fsm.phase_id(),
            Self::KelThuzad(fsm) => fsm.phase_id(),
            Self::Grobbulus(fsm) => fsm.phase_id(),
            Self::None           => 0,
            _                    => 1,
        }
    }

    fn phase_bt(&self) -> Option<&Bt> {
        match self {
            Self::Heigan(fsm)    => fsm.phase_bt(),
            Self::Thaddius(fsm)  => fsm.phase_bt(),
            Self::KelThuzad(fsm) => fsm.phase_bt(),
            Self::Grobbulus(fsm) => fsm.phase_bt(),
            _ => None,
        }
    }

    fn safe_zone_hint(&self) -> u8 {
        match self {
            Self::Heigan(fsm) => fsm.safe_zone(),
            _ => 0,
        }
    }

    fn boss_entry(&self) -> u32 {
        match self {
            Self::AnubRekhan   => ENTRY_ANUB_REKHAN,
            Self::Faerlina     => ENTRY_FAERLINA,
            Self::Maexxna      => ENTRY_MAEXXNA,
            Self::Noth         => ENTRY_NOTH,
            Self::Heigan(_)    => ENTRY_HEIGAN,
            Self::Loatheb      => ENTRY_LOATHEB,
            Self::Razuvious    => ENTRY_RAZUVIOUS,
            Self::Gothik       => ENTRY_GOTHIK,
            Self::FourHorsemen => ENTRY_THANE,
            Self::Patchwerk    => ENTRY_PATCHWERK,
            Self::Grobbulus(_) => ENTRY_GROBBULUS,
            Self::Gluth        => ENTRY_GLUTH,
            Self::Thaddius(_)  => ENTRY_THADDIUS,
            Self::Sapphiron    => ENTRY_SAPPHIRON,
            Self::KelThuzad(_) => ENTRY_KEL_THUZAD,
            Self::None         => 0,
        }
    }

    fn from_entry(entry: u32) -> Self {
        match entry {
            ENTRY_HEIGAN      => Self::Heigan(HeiganFsm::new()),
            ENTRY_THADDIUS    => Self::Thaddius(ThaddiusFsm::new()),
            ENTRY_KEL_THUZAD  => Self::KelThuzad(KelThuzadFsm::new()),
            ENTRY_GROBBULUS   => Self::Grobbulus(GrobbolusFsm::new()),
            ENTRY_ANUB_REKHAN => Self::AnubRekhan,
            ENTRY_FAERLINA    => Self::Faerlina,
            ENTRY_MAEXXNA     => Self::Maexxna,
            ENTRY_NOTH        => Self::Noth,
            ENTRY_LOATHEB     => Self::Loatheb,
            ENTRY_RAZUVIOUS   => Self::Razuvious,
            ENTRY_GOTHIK      => Self::Gothik,
            ENTRY_THANE       => Self::FourHorsemen,
            ENTRY_PATCHWERK   => Self::Patchwerk,
            ENTRY_GLUTH       => Self::Gluth,
            ENTRY_SAPPHIRON   => Self::Sapphiron,
            _                 => Self::None,
        }
    }
}

/// Top-level FSM for the whole Naxxramas instance.
/// Delegates per-boss events and BTs to the appropriate sub-FSM carried
/// inside the `Boss` variant.
pub struct NaxxramasFsm {
    active_boss: Boss,
    simple:      SimpleFsm,
}

impl NaxxramasFsm {
    pub fn new() -> Self {
        Self {
            active_boss: Boss::None,
            simple:      SimpleFsm::new(0),
        }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = Boss::from_entry(entry);
    }
}

impl Default for NaxxramasFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for NaxxramasFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        match &mut self.active_boss {
            Boss::None => self.simple.update(event, boss_hp_pct, time_ms),
            _          => self.active_boss.update(event, boss_hp_pct, time_ms),
        }
    }

    fn phase_id(&self) -> u32 { self.active_boss.phase_id() }

    fn is_active(&self) -> bool { self.active_boss != Boss::None }
    fn is_done(&self)   -> bool { false }

    fn safe_zone_hint(&self) -> u8 { self.active_boss.safe_zone_hint() }

    fn boss_entry(&self) -> u32 { self.active_boss.boss_entry() }

    fn phase_bt(&self) -> Option<&Bt> { self.active_boss.phase_bt() }
}
