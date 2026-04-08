/// Naxxramas — 15 boss encounters across 5 wings.
///
/// Zone ID: 3456.  40-player raid (Vanilla) / 10 and 25-player (`WotLK`).
///
/// Wings:
///   Spider Wing:    Anub'Rekhan, Grand Widow Faerlina, Maexxna
///   Plague Wing:    Noth the Plaguebringer, Heigan the Unclean, Loatheb
///   Military Wing:  Instructor Razuvious, Gothik the Harvester, Four Horsemen
///   Construct Wing: Patchwerk, Grobbulus, Gluth, Thaddius
///   Frostwyrm Lair: Sapphiron, Kel'Thuzad
pub mod grobbulus;
pub mod heigan;
pub mod kel_thuzad;
pub mod thaddius;

pub use grobbulus::GrobbolusFsm;
pub use heigan::HeiganFsm;
pub use kel_thuzad::KelThuzadFsm;
pub use thaddius::ThaddiusFsm;

use super::macros::encounter_dispatch;
use super::{EncounterEvent, EncounterFsm, SimpleFsm};
use crate::engine::bt::Bt;

// ── NPC entry IDs ─────────────────────────────────────────────────────────

pub const ENTRY_ANUB_REKHAN: u32 = 15956;
pub const ENTRY_FAERLINA: u32 = 15953;
pub const ENTRY_MAEXXNA: u32 = 15952;
pub const ENTRY_NOTH: u32 = 15954;
pub const ENTRY_HEIGAN: u32 = 15302;
pub const ENTRY_LOATHEB: u32 = 16011;
pub const ENTRY_RAZUVIOUS: u32 = 16061;
pub const ENTRY_GOTHIK: u32 = 16060;
pub const ENTRY_THANE: u32 = 16063;
pub const ENTRY_PATCHWERK: u32 = 16028;
pub const ENTRY_GROBBULUS: u32 = 15931;
pub const ENTRY_GLUTH: u32 = 15932;
pub const ENTRY_THADDIUS: u32 = 15928;
pub const ENTRY_SAPPHIRON: u32 = 15989;
pub const ENTRY_KEL_THUZAD: u32 = 15990;

encounter_dispatch! {
    #[derive(Clone, PartialEq)]
    pub enum NaxxramasBoss {
        AnubRekhan(SimpleFsm),
        Faerlina(SimpleFsm),
        Maexxna(SimpleFsm),
        Noth(SimpleFsm),
        Heigan(HeiganFsm),
        Loatheb(SimpleFsm),
        Razuvious(SimpleFsm),
        Gothik(SimpleFsm),
        FourHorsemen(SimpleFsm),
        Patchwerk(SimpleFsm),
        Grobbulus(GrobbolusFsm),
        Gluth(SimpleFsm),
        Thaddius(ThaddiusFsm),
        Sapphiron(SimpleFsm),
        KelThuzad(KelThuzadFsm),
    }
}

impl TryFrom<u32> for NaxxramasBoss {
    type Error = ();
    fn try_from(entry: u32) -> Result<Self, Self::Error> {
        match entry {
            ENTRY_ANUB_REKHAN => Ok(Self::AnubRekhan(SimpleFsm::new(entry))),
            ENTRY_FAERLINA => Ok(Self::Faerlina(SimpleFsm::new(entry))),
            ENTRY_MAEXXNA => Ok(Self::Maexxna(SimpleFsm::new(entry))),
            ENTRY_NOTH => Ok(Self::Noth(SimpleFsm::new(entry))),
            ENTRY_HEIGAN => Ok(Self::Heigan(HeiganFsm::default())),
            ENTRY_LOATHEB => Ok(Self::Loatheb(SimpleFsm::new(entry))),
            ENTRY_RAZUVIOUS => Ok(Self::Razuvious(SimpleFsm::new(entry))),
            ENTRY_GOTHIK => Ok(Self::Gothik(SimpleFsm::new(entry))),
            ENTRY_THANE => Ok(Self::FourHorsemen(SimpleFsm::new(entry))),
            ENTRY_PATCHWERK => Ok(Self::Patchwerk(SimpleFsm::new(entry))),
            ENTRY_GROBBULUS => Ok(Self::Grobbulus(GrobbolusFsm::default())),
            ENTRY_GLUTH => Ok(Self::Gluth(SimpleFsm::new(entry))),
            ENTRY_THADDIUS => Ok(Self::Thaddius(ThaddiusFsm::default())),
            ENTRY_SAPPHIRON => Ok(Self::Sapphiron(SimpleFsm::new(entry))),
            ENTRY_KEL_THUZAD => Ok(Self::KelThuzad(KelThuzadFsm::default())),
            _ => Err(()),
        }
    }
}

/// Instance-wide wrapper. Forwards every trait method to the active boss.
///
/// Zone-wide hook location — add a composed
/// `Sel!(boss_bt, zone_wide_bt())` in `phase_bt()` when a real
/// instance-wide mechanic (e.g. Frogger-trash positioning, abomination
/// pull order) is identified.
pub struct NaxxramasFsm {
    active_boss: Option<NaxxramasBoss>,
}

impl NaxxramasFsm {
    pub fn new() -> Self {
        Self { active_boss: None }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = NaxxramasBoss::try_from(entry).ok();
    }
}

impl Default for NaxxramasFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for NaxxramasFsm {
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

    fn safe_zone_hint(&self) -> u8 {
        self.active_boss.as_ref().map_or(0, |b| b.safe_zone_hint())
    }

    fn boss_entry(&self) -> u32 {
        self.active_boss.as_ref().map_or(0, |b| b.boss_entry())
    }

    fn phase_bt(&self, fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        self.active_boss.as_ref().and_then(|b| b.phase_bt(fsm))
    }
}
