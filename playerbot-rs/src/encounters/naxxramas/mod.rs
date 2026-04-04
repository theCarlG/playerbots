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

mod heigan;
mod thaddius;
mod kel_thuzad;
mod grobbulus;

pub use heigan::HeiganFsm;
pub use thaddius::ThaddiusFsm;
pub use kel_thuzad::KelThuzadFsm;
pub use grobbulus::GrobbolusFsm;

use super::{EncounterEvent, EncounterFsm, SimpleFsm};

// ── NPC entry IDs ─────────────────────────────────────────────────────────

pub const ENTRY_ANUB_REKHAN:     u32 = 15956;
pub const ENTRY_FAERLINA:        u32 = 15953;
pub const ENTRY_MAEXXNA:         u32 = 15952;
pub const ENTRY_NOTH:            u32 = 15954;
pub const ENTRY_HEIGAN:          u32 = 15302;
pub const ENTRY_LOATHEB:         u32 = 16011;
pub const ENTRY_RAZUVIOUS:       u32 = 16061;
pub const ENTRY_GOTHIK:          u32 = 16060;
pub const ENTRY_THANE:           u32 = 16063;  // Four Horsemen — Thane Korth'azz
pub const ENTRY_PATCHWERK:       u32 = 16028;
pub const ENTRY_GROBBULUS:       u32 = 15931;
pub const ENTRY_GLUTH:           u32 = 15932;
pub const ENTRY_THADDIUS:        u32 = 15928;
pub const ENTRY_SAPPHIRON:       u32 = 15989;
pub const ENTRY_KEL_THUZAD:      u32 = 15990;

#[derive(Clone, Copy, PartialEq)]
enum ActiveBoss {
    None,
    AnubRekhan,
    Faerlina,
    Maexxna,
    Noth,
    Heigan,
    Loatheb,
    Razuvious,
    Gothik,
    FourHorsemen,
    Patchwerk,
    Grobbulus,
    Gluth,
    Thaddius,
    Sapphiron,
    KelThuzad,
}

/// Top-level FSM for the whole Naxxramas instance.
/// Delegates per-boss events to the appropriate sub-FSM.
pub struct NaxxramasFsm {
    active_boss: ActiveBoss,
    heigan:      HeiganFsm,
    thaddius:    ThaddiusFsm,
    kel_thuzad:  KelThuzadFsm,
    grobbulus:   GrobbolusFsm,
    simple:      SimpleFsm,
}

impl NaxxramasFsm {
    pub fn new() -> Self {
        Self {
            active_boss: ActiveBoss::None,
            heigan:      HeiganFsm::new(),
            thaddius:    ThaddiusFsm::new(),
            kel_thuzad:  KelThuzadFsm::new(),
            grobbulus:   GrobbolusFsm::new(),
            simple:      SimpleFsm::new(0),
        }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = match entry {
            ENTRY_ANUB_REKHAN   => ActiveBoss::AnubRekhan,
            ENTRY_FAERLINA      => ActiveBoss::Faerlina,
            ENTRY_MAEXXNA       => ActiveBoss::Maexxna,
            ENTRY_NOTH          => ActiveBoss::Noth,
            ENTRY_HEIGAN        => ActiveBoss::Heigan,
            ENTRY_LOATHEB       => ActiveBoss::Loatheb,
            ENTRY_RAZUVIOUS     => ActiveBoss::Razuvious,
            ENTRY_GOTHIK        => ActiveBoss::Gothik,
            ENTRY_THANE         => ActiveBoss::FourHorsemen,
            ENTRY_PATCHWERK     => ActiveBoss::Patchwerk,
            ENTRY_GROBBULUS     => ActiveBoss::Grobbulus,
            ENTRY_GLUTH         => ActiveBoss::Gluth,
            ENTRY_THADDIUS      => ActiveBoss::Thaddius,
            ENTRY_SAPPHIRON     => ActiveBoss::Sapphiron,
            ENTRY_KEL_THUZAD    => ActiveBoss::KelThuzad,
            _                   => return,
        };
    }
}

impl Default for NaxxramasFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for NaxxramasFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        match self.active_boss {
            ActiveBoss::Heigan    => self.heigan.update(event, boss_hp_pct, time_ms),
            ActiveBoss::Thaddius  => self.thaddius.update(event, boss_hp_pct, time_ms),
            ActiveBoss::KelThuzad => self.kel_thuzad.update(event, boss_hp_pct, time_ms),
            ActiveBoss::Grobbulus => self.grobbulus.update(event, boss_hp_pct, time_ms),
            _                     => self.simple.update(event, boss_hp_pct, time_ms),
        }
    }

    fn phase_id(&self) -> u32 {
        match self.active_boss {
            ActiveBoss::None      => 0,
            ActiveBoss::Heigan    => self.heigan.phase_id(),
            ActiveBoss::Thaddius  => self.thaddius.phase_id(),
            ActiveBoss::KelThuzad => self.kel_thuzad.phase_id(),
            ActiveBoss::Grobbulus => self.grobbulus.phase_id(),
            _                     => self.simple.phase_id(),
        }
    }

    fn is_active(&self) -> bool { self.active_boss != ActiveBoss::None }
    fn is_done(&self)   -> bool { false }

    fn boss_entry(&self) -> u32 {
        match self.active_boss {
            ActiveBoss::AnubRekhan   => ENTRY_ANUB_REKHAN,
            ActiveBoss::Faerlina     => ENTRY_FAERLINA,
            ActiveBoss::Maexxna      => ENTRY_MAEXXNA,
            ActiveBoss::Noth         => ENTRY_NOTH,
            ActiveBoss::Heigan       => ENTRY_HEIGAN,
            ActiveBoss::Loatheb      => ENTRY_LOATHEB,
            ActiveBoss::Razuvious    => ENTRY_RAZUVIOUS,
            ActiveBoss::Gothik       => ENTRY_GOTHIK,
            ActiveBoss::FourHorsemen => ENTRY_THANE,
            ActiveBoss::Patchwerk    => ENTRY_PATCHWERK,
            ActiveBoss::Grobbulus    => ENTRY_GROBBULUS,
            ActiveBoss::Gluth        => ENTRY_GLUTH,
            ActiveBoss::Thaddius     => ENTRY_THADDIUS,
            ActiveBoss::Sapphiron    => ENTRY_SAPPHIRON,
            ActiveBoss::KelThuzad    => ENTRY_KEL_THUZAD,
            ActiveBoss::None         => 0,
        }
    }
}
