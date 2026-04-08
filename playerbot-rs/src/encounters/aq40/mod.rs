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
use super::macros::encounter_dispatch;
use super::{EncounterEvent, EncounterFsm, SimpleFsm};
use crate::engine::bt::Bt;

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
pub const ENTRY_CTHUN_EYE: u32 = 15589;

// Variant notes:
//   BugTrio — Kri / Vem / Yauj collapsed into a single variant as a first-pass
//             scaffold. A future `BugTrioFsm` will track all three HPs.
//   Twins   — Veklor / Veknilash collapsed; real fight requires swap aggro.
//   Cthun   — Eye of C'thun (phase 1) and C'thun body (phase 2) collapsed;
//             a future bespoke `CthunFsm` replaces the payload.
encounter_dispatch! {
    #[derive(Clone, PartialEq)]
    pub enum Aq40Boss {
        Skeram(SimpleFsm),
        BugTrio(SimpleFsm),
        Sartura(SimpleFsm),
        Fankriss(SimpleFsm),
        Viscidus(SimpleFsm),
        Huhuran(SimpleFsm),
        Twins(SimpleFsm),
        Ouro(SimpleFsm),
        Cthun(SimpleFsm),
    }
}

impl TryFrom<u32> for Aq40Boss {
    type Error = ();
    fn try_from(entry: u32) -> Result<Self, Self::Error> {
        match entry {
            ENTRY_SKERAM => Ok(Self::Skeram(SimpleFsm::new(entry))),
            ENTRY_KRI | ENTRY_VEM | ENTRY_YAUJ => Ok(Self::BugTrio(SimpleFsm::new(entry))),
            ENTRY_SARTURA => Ok(Self::Sartura(SimpleFsm::new(entry))),
            ENTRY_FANKRISS => Ok(Self::Fankriss(SimpleFsm::new(entry))),
            ENTRY_VISCIDUS => Ok(Self::Viscidus(SimpleFsm::new(entry))),
            ENTRY_HUHURAN => Ok(Self::Huhuran(SimpleFsm::new(entry))),
            ENTRY_EMPEROR_VEKLOR | ENTRY_EMPEROR_VEKNILASH => {
                Ok(Self::Twins(SimpleFsm::new(entry)))
            }
            ENTRY_OURO => Ok(Self::Ouro(SimpleFsm::new(entry))),
            ENTRY_CTHUN_EYE | ENTRY_CTHUN => Ok(Self::Cthun(SimpleFsm::new(entry))),
            _ => Err(()),
        }
    }
}

/// Instance-wide wrapper. Zone-wide hook location — add composed
/// `Sel!(boss_bt, zone_wide_bt())` in `phase_bt()` when a real
/// instance-wide mechanic is identified.
pub struct Aq40Fsm {
    active_boss: Option<Aq40Boss>,
}

impl Aq40Fsm {
    pub fn new() -> Self {
        Self { active_boss: None }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = Aq40Boss::try_from(entry).ok();
    }
}

impl Default for Aq40Fsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for Aq40Fsm {
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

    fn phase_bt(&self, fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        self.active_boss.as_ref().and_then(|b| b.phase_bt(fsm))
    }
}
