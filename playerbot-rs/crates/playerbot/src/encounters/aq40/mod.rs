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
pub mod cthun;
pub mod fankriss;
pub mod huhuran;
pub mod ouro;
pub mod sartura;
pub mod twins;
pub mod viscidus;

use super::macros::encounter_dispatch;
use super::{EncounterEvent, EncounterFsm, SimpleFsm};
use crate::engine::bt::Bt;
pub use cthun::CthunFsm;
pub use fankriss::FankrissFsm;
pub use huhuran::HuhuranFsm;
pub use ouro::OuroFsm;
pub use sartura::SarturaFsm;
pub use twins::TwinsFsm;
pub use viscidus::ViscidusFsm;

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

// ── Add / tentacle entry IDs (verified against the world DB) ──────────────
/// Sartura's adds — also whirlwind, must be cleared.
pub const ENTRY_SARTURA_ROYAL_GUARD: u32 = 15984;
/// Fankriss adds.
pub const ENTRY_SPAWN_OF_FANKRISS: u32 = 15630;
pub const ENTRY_VEKNISS_HATCHLING: u32 = 15962;
/// Ouro's scarab swarm.
pub const ENTRY_OURO_SCARAB: u32 = 15718;
/// Viscidus shatter-globs — kill fast or they recombine.
pub const ENTRY_GLOB_OF_VISCIDUS: u32 = 15667;
/// C'thun tentacles.
pub const ENTRY_EYE_TENTACLE: u32 = 15726;
pub const ENTRY_CLAW_TENTACLE: u32 = 15725;
pub const ENTRY_GIANT_EYE_TENTACLE: u32 = 15334;
pub const ENTRY_GIANT_CLAW_TENTACLE: u32 = 15728;
/// Flesh Tentacle inside the stomach (Phase 2).
pub const ENTRY_FLESH_TENTACLE: u32 = 15802;

// Variant notes:
//   Sartura/Fankriss/Ouro/Viscidus — real add-focus FSMs.
//   Huhuran — real FSM (hunter Tranq Shot on Frenzy).
//   Twins   — real FSM: melee → Vek'nilash, ranged → Vek'lor (both entries map
//             to `TwinsFsm`, which keeps the entry it was created with). Tank
//             swap/spacing is still raid coordination, not scripted.
//   Cthun   — real FSM: focus tentacles in priority (both Eye and body entries
//             map to `CthunFsm`). Eye-beam/Dark-Glare dodging not scripted.
//   Skeram  — SimpleFsm: Arcane Explosion / True Fulfillment (MC) have no clean
//             per-bot signal; the illusion copies aren't distinguishable.
//   BugTrio — SimpleFsm: Kri/Vem/Yauj need HP-balanced kills (a raid-wide burst
//             coordination a per-bot focus would fight, not improve).
encounter_dispatch! {
    #[derive(Clone, PartialEq)]
    pub enum Aq40Boss {
        Skeram(SimpleFsm),
        BugTrio(SimpleFsm),
        Sartura(SarturaFsm),
        Fankriss(FankrissFsm),
        Viscidus(ViscidusFsm),
        Huhuran(HuhuranFsm),
        Twins(TwinsFsm),
        Ouro(OuroFsm),
        Cthun(CthunFsm),
    }
}

impl TryFrom<u32> for Aq40Boss {
    type Error = ();
    fn try_from(entry: u32) -> Result<Self, Self::Error> {
        match entry {
            ENTRY_SKERAM => Ok(Self::Skeram(SimpleFsm::new(entry))),
            ENTRY_KRI | ENTRY_VEM | ENTRY_YAUJ => Ok(Self::BugTrio(SimpleFsm::new(entry))),
            ENTRY_SARTURA => Ok(Self::Sartura(SarturaFsm::default())),
            ENTRY_FANKRISS => Ok(Self::Fankriss(FankrissFsm::default())),
            ENTRY_VISCIDUS => Ok(Self::Viscidus(ViscidusFsm::default())),
            ENTRY_HUHURAN => Ok(Self::Huhuran(HuhuranFsm::default())),
            ENTRY_EMPEROR_VEKLOR | ENTRY_EMPEROR_VEKNILASH => {
                Ok(Self::Twins(TwinsFsm::new(entry)))
            }
            ENTRY_OURO => Ok(Self::Ouro(OuroFsm::default())),
            ENTRY_CTHUN_EYE | ENTRY_CTHUN => Ok(Self::Cthun(CthunFsm::new(entry))),
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
        if self
            .active_boss
            .as_ref().is_none_or(|b| b.boss_entry() != entry)
        {
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
