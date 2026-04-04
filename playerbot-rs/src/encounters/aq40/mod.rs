/// Temple of Ahn'Qiraj (AQ40) — 9 boss encounters.
///
/// Zone ID: 3428.  40-player raid.
///
/// Bosses:
///   1. The Prophet Skeram       (entry 15263) — 3 illusion copies
///   2. Silithid Royalty Bug Trio (entries 15544/15543/15538) — three at once
///   3. Battleguard Sartura       (entry 15516) — whirlwind (run out)
///   4. Fankriss the Unyielding   (entry 15510) — worm adds
///   5. Viscidus                  (entry 15299) — freeze then shatter (frost/physical)
///   6. Princess Huhuran          (entry 15509) — nature resist check + frenzy
///   7. Twin Emperors             (entries 15276/15275) — swap mechanic (swap tanks)
///   8. Ouro                      (entry 15517) — burrowing mechanic
///   9. C'thun                    (entry 15727) — tentacles, eye beam, stomach
use super::{EncounterEvent, EncounterFsm, SimpleFsm};

pub const ENTRY_SKERAM: u32 = 15263;
pub const ENTRY_KRI: u32 = 15544; // Bug Trio
pub const ENTRY_VEM: u32 = 15543;
pub const ENTRY_YAUJ: u32 = 15538;
pub const ENTRY_SARTURA: u32 = 15516;
pub const ENTRY_FANKRISS: u32 = 15510;
pub const ENTRY_VISCIDUS: u32 = 15299;
pub const ENTRY_HUHURAN: u32 = 15509;
pub const ENTRY_EMPEROR_VEKLOR: u32 = 15276;
pub const ENTRY_EMPEROR_VEKNILASH: u32 = 15275;
pub const ENTRY_OURO: u32 = 15517;
pub const ENTRY_CTHUN: u32 = 15727;

/// C'thun phase — Eye of C'thun (Phase 1) vs. C'thun body (Phase 2).
pub const ENTRY_CTHUN_EYE: u32 = 15589; // Phase 1 target

pub struct Aq40Fsm {
    simple: SimpleFsm,
}

impl Aq40Fsm {
    pub fn new() -> Self {
        Self {
            simple: SimpleFsm::new(0),
        }
    }
}

impl Default for Aq40Fsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for Aq40Fsm {
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
