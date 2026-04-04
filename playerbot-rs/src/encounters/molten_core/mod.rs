pub mod baron_geddon;
pub mod garr;
pub mod lucifron;
pub mod magmadar;
/// Molten Core — 10 boss encounters.
///
/// Zone ID: 2717.  40-player raid.
pub mod ragnaros;
pub mod shazzrah;

use super::{EncounterEvent, EncounterFsm, SimpleFsm};
use crate::engine::bt::Bt;
pub use baron_geddon::BaronGeddonFsm;
pub use garr::GarrFsm;
pub use lucifron::LucifronFsm;
pub use magmadar::MagmadarFsm;
pub use ragnaros::RagnarosFsm;
pub use shazzrah::ShazzrahFsm;

// ── NPC entry IDs ─────────────────────────────────────────────────────────

pub const ENTRY_LUCIFRON: u32 = 12118;
pub const ENTRY_MAGMADAR: u32 = 11982;
pub const ENTRY_GEHENNAS: u32 = 12259;
pub const ENTRY_GARR: u32 = 12057;
pub const ENTRY_BARON_GEDDON: u32 = 12056;
pub const ENTRY_SHAZZRAH: u32 = 12264;
pub const ENTRY_SULFURON: u32 = 12098;
pub const ENTRY_GOLEMAGG: u32 = 11988;
pub const ENTRY_MAJORDOMO: u32 = 12018;
pub const ENTRY_RAGNAROS: u32 = 11502;

// Spell IDs for in-zone mechanics (shared across bosses)
pub const SPELL_FIRE_PROTECTION_POTION: crate::ffi::SpellId = crate::ffi::SpellId(17543);

pub struct MoltenCoreFsm {
    active_boss: Boss,
    simple: SimpleFsm,
}

#[derive(Clone, PartialEq)]
enum Boss {
    None,
    Lucifron(LucifronFsm),
    Magmadar(MagmadarFsm),
    Gehennas,
    Garr(GarrFsm),
    BaronGeddon(BaronGeddonFsm),
    Shazzrah(ShazzrahFsm),
    Sulfuron,
    Golemagg,
    Majordomo,
    Ragnaros(RagnarosFsm),
}

impl Boss {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        match self {
            Self::Ragnaros(fsm)    => fsm.update(event, boss_hp_pct, time_ms),
            Self::BaronGeddon(fsm) => fsm.update(event, boss_hp_pct, time_ms),
            Self::Magmadar(fsm)    => fsm.update(event, boss_hp_pct, time_ms),
            Self::Lucifron(fsm)    => fsm.update(event, boss_hp_pct, time_ms),
            Self::Garr(fsm)        => fsm.update(event, boss_hp_pct, time_ms),
            Self::Shazzrah(fsm)    => fsm.update(event, boss_hp_pct, time_ms),
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match &self {
            Self::Ragnaros(fsm)    => fsm.phase_id(),
            Self::BaronGeddon(_)   => 10,
            Self::Magmadar(_)      => 11,
            Self::Lucifron(_)      => 12,
            Self::Garr(_)          => 13,
            Self::Shazzrah(_)      => 14,
            Self::None => 0,
            _ => 1,
        }
    }

    fn phase_bt(&self) -> Option<&Bt> {
        match self {
            Self::Ragnaros(fsm)    => fsm.phase_bt(),
            Self::BaronGeddon(fsm) => fsm.phase_bt(),
            Self::Magmadar(fsm)    => fsm.phase_bt(),
            Self::Lucifron(fsm)    => fsm.phase_bt(),
            Self::Garr(fsm)        => fsm.phase_bt(),
            Self::Shazzrah(fsm)    => fsm.phase_bt(),
            _ => None,
        }
    }

    fn from_entry(entry: u32) -> Self {
        match entry {
            ENTRY_RAGNAROS     => Self::Ragnaros(RagnarosFsm::new()),
            ENTRY_BARON_GEDDON => Self::BaronGeddon(BaronGeddonFsm::new()),
            ENTRY_MAGMADAR     => Self::Magmadar(MagmadarFsm::new()),
            ENTRY_LUCIFRON     => Self::Lucifron(LucifronFsm::new()),
            ENTRY_GARR         => Self::Garr(GarrFsm::new()),
            ENTRY_SHAZZRAH     => Self::Shazzrah(ShazzrahFsm::new()),
            ENTRY_GEHENNAS     => Self::Gehennas,
            ENTRY_SULFURON     => Self::Sulfuron,
            ENTRY_GOLEMAGG     => Self::Golemagg,
            ENTRY_MAJORDOMO    => Self::Majordomo,
            _ => Self::None,
        }
    }

    fn as_entry(&self) -> u32 {
        match self {
            Self::Ragnaros(_)    => ENTRY_RAGNAROS,
            Self::BaronGeddon(_) => ENTRY_BARON_GEDDON,
            Self::Magmadar(_)    => ENTRY_MAGMADAR,
            Self::Lucifron(_)    => ENTRY_LUCIFRON,
            Self::Garr(_)        => ENTRY_GARR,
            Self::Shazzrah(_)    => ENTRY_SHAZZRAH,
            Self::Gehennas       => ENTRY_GEHENNAS,
            Self::Sulfuron       => ENTRY_SULFURON,
            Self::Golemagg       => ENTRY_GOLEMAGG,
            Self::Majordomo      => ENTRY_MAJORDOMO,
            Self::None => 0,
        }
    }
}

impl MoltenCoreFsm {
    pub fn new() -> Self {
        Self {
            active_boss: Boss::None,
            simple: SimpleFsm::new(0),
        }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = Boss::from_entry(entry);
    }
}

impl Default for MoltenCoreFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for MoltenCoreFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        match &mut self.active_boss {
            Boss::None => self.simple.update(event, boss_hp_pct, time_ms),
            _ => self.active_boss.update(event, boss_hp_pct, time_ms),
        }
    }

    fn phase_id(&self) -> u32 {
        self.active_boss.phase_id()
    }

    fn is_active(&self) -> bool {
        self.active_boss != Boss::None
    }
    fn is_done(&self) -> bool {
        false
    }

    fn boss_entry(&self) -> u32 {
        self.active_boss.as_entry()
    }

    fn phase_bt(&self) -> Option<&Bt> {
        self.active_boss.phase_bt()
    }
}
