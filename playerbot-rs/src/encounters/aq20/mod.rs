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
use super::{EncounterEvent, EncounterFsm, SimpleFsm};

pub const ENTRY_KURINNAXX: u32 = 15348;
pub const ENTRY_RAJAXX: u32 = 15341;
pub const ENTRY_MOAM: u32 = 15340;
pub const ENTRY_BURU: u32 = 15370;
pub const ENTRY_AYAMISS: u32 = 15369;
pub const ENTRY_OSSIRIAN: u32 = 15339;

pub struct Aq20Fsm {
    simple: SimpleFsm,
}

impl Aq20Fsm {
    pub fn new() -> Self {
        Self {
            simple: SimpleFsm::new(0),
        }
    }
}

impl Default for Aq20Fsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for Aq20Fsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp: f32, time: u64) {
        self.simple.update(event, boss_hp, time);
    }
    fn phase_id(&self) -> u32 {
        self.simple.phase_id()
    }
    fn is_active(&self) -> bool {
        self.simple.is_active()
    }
    fn is_done(&self) -> bool {
        self.simple.is_done()
    }
    fn boss_entry(&self) -> u32 {
        self.simple.boss_entry()
    }
}
