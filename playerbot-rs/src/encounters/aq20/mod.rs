/// Ruins of Ahn'Qiraj (AQ20) — 6 boss encounters.
///
/// Zone ID: 3429.  20-player raid.
///
/// Bosses:
///   1. Kurinnaxx         (entry 15348) — sand traps, no aggro reset
///   2. General Rajaxx    (entry 15341) — 7 add waves before Rajaxx himself
///   3. Moam              (entry 15340) — drain mana + stone elemental adds
///   4. Buru the Gorger   (entry 15370) — egg mechanic (explode eggs)
///   5. Ayamiss the Hunter (entry 15369) — air phase + adds
///   6. Ossirian the Unscarred (entry 15339) — weakness crystals required
use super::macros::encounter_dispatch;
use super::{EncounterEvent, EncounterFsm, SimpleFsm};
use crate::engine::bt::Bt;

pub const ENTRY_KURINNAXX: u32 = 15348;
pub const ENTRY_RAJAXX: u32 = 15341;
pub const ENTRY_MOAM: u32 = 15340;
pub const ENTRY_BURU: u32 = 15370;
pub const ENTRY_AYAMISS: u32 = 15369;
pub const ENTRY_OSSIRIAN: u32 = 15339;

encounter_dispatch! {
    #[derive(Clone, PartialEq)]
    pub enum Aq20Boss {
        Kurinnaxx(SimpleFsm),
        Rajaxx(SimpleFsm),
        Moam(SimpleFsm),
        Buru(SimpleFsm),
        Ayamiss(SimpleFsm),
        Ossirian(SimpleFsm),
    }
}

impl TryFrom<u32> for Aq20Boss {
    type Error = ();
    fn try_from(entry: u32) -> Result<Self, Self::Error> {
        match entry {
            ENTRY_KURINNAXX => Ok(Self::Kurinnaxx(SimpleFsm::new(entry))),
            ENTRY_RAJAXX => Ok(Self::Rajaxx(SimpleFsm::new(entry))),
            ENTRY_MOAM => Ok(Self::Moam(SimpleFsm::new(entry))),
            ENTRY_BURU => Ok(Self::Buru(SimpleFsm::new(entry))),
            ENTRY_AYAMISS => Ok(Self::Ayamiss(SimpleFsm::new(entry))),
            ENTRY_OSSIRIAN => Ok(Self::Ossirian(SimpleFsm::new(entry))),
            _ => Err(()),
        }
    }
}

/// Instance-wide wrapper. Zone-wide hook location — add composed
/// `Sel!(boss_bt, zone_wide_bt())` in `phase_bt()` when an instance-wide
/// mechanic (e.g. Ossirian weakness-crystal activation) is identified.
pub struct Aq20Fsm {
    active_boss: Option<Aq20Boss>,
}

impl Aq20Fsm {
    pub fn new() -> Self {
        Self { active_boss: None }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = Aq20Boss::try_from(entry).ok();
    }
}

impl Default for Aq20Fsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for Aq20Fsm {
    fn set_boss_entry(&mut self, entry: u32) {
        if !self.active_boss.as_ref().is_some_and(|b| b.boss_entry() == entry) {
            self.set_active_boss_by_entry(entry);
        }
    }

    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        if let Some(boss) = &mut self.active_boss {
            boss.update(event, boss_hp_pct, time_ms);
        }
    }

    fn phase_id(&self) -> u32 {
        self.active_boss.as_ref().map_or(0, |b| b.phase_id())
    }

    fn is_active(&self) -> bool {
        self.active_boss.is_some()
    }

    fn is_done(&self) -> bool {
        self.active_boss.as_ref().is_some_and(|b| b.is_done())
    }

    fn boss_entry(&self) -> u32 {
        self.active_boss.as_ref().map_or(0, |b| b.boss_entry())
    }

    fn phase_bt(&self) -> Option<Bt> {
        self.active_boss.as_ref().and_then(|b| b.phase_bt())
    }
}
