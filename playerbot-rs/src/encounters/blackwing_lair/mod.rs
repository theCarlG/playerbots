/// Blackwing Lair — 8 boss encounters.
///
/// Zone ID: 2677.  40-player raid.
///
/// Bosses in order:
///   1. Razorgore the Untamed    (entry 12435)
///   2. Vaelastrasz the Corrupt  (entry 13020)
///   3. Broodlord Lashlayer      (entry 12017)
///   4. Firemaw                  (entry 11983)
///   5. Ebonroc                  (entry 14601)
///   6. Flamegor                 (entry 14610)
///   7. Chromaggus               (entry 14020)
///   8. Nefarian                 (entry 11583)

pub mod broodlord;
pub mod chromaggus;
pub mod nefarian;
pub mod vaelastrasz;

use super::{EncounterEvent, EncounterFsm, SimpleFsm};
use crate::engine::bt::Bt;
pub use broodlord::BroodlordFsm;
pub use chromaggus::ChromaggusFsm;
pub use nefarian::NefarianFsm;
pub use vaelastrasz::VaelastraszFsm;

pub const ENTRY_RAZORGORE:   u32 = 12435;
pub const ENTRY_VAELASTRASZ: u32 = 13020;
pub const ENTRY_BROODLORD:   u32 = 12017;
pub const ENTRY_FIREMAW:     u32 = 11983;
pub const ENTRY_EBONROC:     u32 = 14601;
pub const ENTRY_FLAMEGOR:    u32 = 14610;
pub const ENTRY_CHROMAGGUS:  u32 = 14020;
pub const ENTRY_NEFARIAN:    u32 = 11583;

#[derive(Clone, PartialEq)]
enum Boss {
    None,
    Razorgore,
    Vaelastrasz(VaelastraszFsm),
    Broodlord(BroodlordFsm),
    Firemaw, Ebonroc, Flamegor,
    Chromaggus(ChromaggusFsm),
    Nefarian(NefarianFsm),
}

impl Boss {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        match self {
            Self::Nefarian(fsm)    => fsm.update(event, boss_hp_pct, time_ms),
            Self::Vaelastrasz(fsm) => fsm.update(event, boss_hp_pct, time_ms),
            Self::Broodlord(fsm)   => fsm.update(event, boss_hp_pct, time_ms),
            Self::Chromaggus(fsm)  => fsm.update(event, boss_hp_pct, time_ms),
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self {
            Self::Nefarian(fsm)    => fsm.phase_id(),
            Self::Vaelastrasz(fsm) => fsm.phase_id(),
            Self::Broodlord(fsm)   => fsm.phase_id(),
            Self::Chromaggus(fsm)  => fsm.phase_id(),
            Self::None             => 0,
            _                      => 1,
        }
    }

    fn phase_bt(&self) -> Option<&Bt> {
        match self {
            Self::Nefarian(fsm)    => fsm.phase_bt(),
            Self::Vaelastrasz(fsm) => fsm.phase_bt(),
            Self::Broodlord(fsm)   => fsm.phase_bt(),
            Self::Chromaggus(fsm)  => fsm.phase_bt(),
            _ => None,
        }
    }

    fn boss_entry(&self) -> u32 {
        match self {
            Self::Razorgore      => ENTRY_RAZORGORE,
            Self::Vaelastrasz(_) => ENTRY_VAELASTRASZ,
            Self::Broodlord(_)   => ENTRY_BROODLORD,
            Self::Firemaw        => ENTRY_FIREMAW,
            Self::Ebonroc        => ENTRY_EBONROC,
            Self::Flamegor       => ENTRY_FLAMEGOR,
            Self::Chromaggus(_)  => ENTRY_CHROMAGGUS,
            Self::Nefarian(_)    => ENTRY_NEFARIAN,
            Self::None           => 0,
        }
    }

    fn from_entry(entry: u32) -> Self {
        match entry {
            ENTRY_NEFARIAN    => Self::Nefarian(NefarianFsm::new()),
            ENTRY_RAZORGORE   => Self::Razorgore,
            ENTRY_VAELASTRASZ => Self::Vaelastrasz(VaelastraszFsm::new()),
            ENTRY_BROODLORD   => Self::Broodlord(BroodlordFsm::new()),
            ENTRY_FIREMAW     => Self::Firemaw,
            ENTRY_EBONROC     => Self::Ebonroc,
            ENTRY_FLAMEGOR    => Self::Flamegor,
            ENTRY_CHROMAGGUS  => Self::Chromaggus(ChromaggusFsm::new()),
            _                 => Self::None,
        }
    }
}

pub struct BlackwingLairFsm {
    active_boss: Boss,
    simple:      SimpleFsm,
}

impl BlackwingLairFsm {
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

impl Default for BlackwingLairFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for BlackwingLairFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        match &mut self.active_boss {
            Boss::None => self.simple.update(event, boss_hp_pct, time_ms),
            _          => self.active_boss.update(event, boss_hp_pct, time_ms),
        }
    }

    fn phase_id(&self) -> u32 { self.active_boss.phase_id() }

    fn is_active(&self) -> bool { self.active_boss != Boss::None }
    fn is_done(&self)   -> bool { false }

    fn boss_entry(&self) -> u32 { self.active_boss.boss_entry() }

    fn phase_bt(&self) -> Option<&Bt> { self.active_boss.phase_bt() }
}
