/// Zul'Gurub — 10 boss encounters (5 High Priests + 5 aspect bosses).
///
/// Zone ID: 1977.  20-player raid.
///
/// Bosses:
///   1. High Priestess Jeklik    (entry 14517) — bat phase + heal interrupt
///   2. High Priest Venoxis      (entry 14507) — snake form + Holy Wrath
///   3. High Priestess Mar'li    (entry 14510) — spider adds
///   4. Bloodlord Mandokir       (entry 11382) — Charge + Threatening Gaze
///   5. Edge of Madness           (entry varies) — optional summon boss
///   6. High Priest Thekal       (entry 14509) — zealot trio → tiger form
///   7. Gahz'ranka               (entry 15114) — optional fishing boss
///   8. High Priestess Arlokk    (entry 14515) — vanish + panther adds
///   9. Jin'do the Hexxer        (entry 11380) — totems + hex
///  10. Hakkar the Soulflayer    (entry 14834) — blood siphon mechanic
use super::macros::encounter_dispatch;
use super::{EncounterEvent, EncounterFsm, SimpleFsm};
use crate::engine::bt::Bt;

pub const ENTRY_JEKLIK: u32 = 14517;
pub const ENTRY_VENOXIS: u32 = 14507;
pub const ENTRY_MARLI: u32 = 14510;
pub const ENTRY_MANDOKIR: u32 = 11382;
pub const ENTRY_THEKAL: u32 = 14509;
pub const ENTRY_GAHZRANKA: u32 = 15114;
pub const ENTRY_ARLOKK: u32 = 14515;
pub const ENTRY_JINDO: u32 = 11380;
pub const ENTRY_HAKKAR: u32 = 14834;

encounter_dispatch! {
    #[derive(Clone, PartialEq)]
    pub enum ZulGurubBoss {
        Jeklik(SimpleFsm),
        Venoxis(SimpleFsm),
        Marli(SimpleFsm),
        Mandokir(SimpleFsm),
        Thekal(SimpleFsm),
        Gahzranka(SimpleFsm),
        Arlokk(SimpleFsm),
        Jindo(SimpleFsm),
        Hakkar(SimpleFsm),
    }
}

impl TryFrom<u32> for ZulGurubBoss {
    type Error = ();
    fn try_from(entry: u32) -> Result<Self, Self::Error> {
        match entry {
            ENTRY_JEKLIK => Ok(Self::Jeklik(SimpleFsm::new(entry))),
            ENTRY_VENOXIS => Ok(Self::Venoxis(SimpleFsm::new(entry))),
            ENTRY_MARLI => Ok(Self::Marli(SimpleFsm::new(entry))),
            ENTRY_MANDOKIR => Ok(Self::Mandokir(SimpleFsm::new(entry))),
            ENTRY_THEKAL => Ok(Self::Thekal(SimpleFsm::new(entry))),
            ENTRY_GAHZRANKA => Ok(Self::Gahzranka(SimpleFsm::new(entry))),
            ENTRY_ARLOKK => Ok(Self::Arlokk(SimpleFsm::new(entry))),
            ENTRY_JINDO => Ok(Self::Jindo(SimpleFsm::new(entry))),
            ENTRY_HAKKAR => Ok(Self::Hakkar(SimpleFsm::new(entry))),
            _ => Err(()),
        }
    }
}

pub struct ZulGurubFsm {
    active_boss: Option<ZulGurubBoss>,
}

impl ZulGurubFsm {
    pub fn new() -> Self {
        Self { active_boss: None }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = ZulGurubBoss::try_from(entry).ok();
    }
}

impl Default for ZulGurubFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for ZulGurubFsm {
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
