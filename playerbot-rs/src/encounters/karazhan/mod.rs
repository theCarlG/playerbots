/// Karazhan — 11 boss encounters (TBC only).
///
/// Zone ID: 3457.  10-player raid.  Feature-gated on `tbc` or `wotlk`.
///
/// Bosses:
///   1. Attumen the Huntsman   (entry 15550 + 16152 Midnight)
///   2. Moroes                 (entry 15687) — adds, vanish, garrote
///   3. Maiden of Virtue       (entry 16457) — Holy Ground
///   4. Opera Event            (variable) — 3 possible events
///   5. The Curator            (entry 15691) — Hateful Bolt, add sparks
///   6. Terestian Illhoof      (entry 15688) — imp adds
///   7. Shade of Aran          (entry 16524) — no tank, elementals, blizzard
///   8. Netherspite             (entry 15689) — beam portals, phase swap
///   9. Chess Event             (entry 21227) — scripted puzzle
///  10. Prince Malchezaar       (entry 15690) — 3 phases, infernal
///  11. Nightbane               (entry 17225) — air/ground phases

#[cfg(any(feature = "tbc", feature = "wotlk"))]
use super::{EncounterEvent, EncounterFsm, SimpleFsm};

pub const ENTRY_ATTUMEN: u32 = 15550;
pub const ENTRY_MOROES: u32 = 15687;
pub const ENTRY_MAIDEN: u32 = 16457;
pub const ENTRY_CURATOR: u32 = 15691;
pub const ENTRY_ILLHOOF: u32 = 15688;
pub const ENTRY_SHADE_OF_ARAN: u32 = 16524;
pub const ENTRY_NETHERSPITE: u32 = 15689;
pub const ENTRY_PRINCE: u32 = 15690;
pub const ENTRY_NIGHTBANE: u32 = 17225;

#[cfg(any(feature = "tbc", feature = "wotlk"))]
pub struct KarazhanFsm {
    simple: SimpleFsm,
}

#[cfg(any(feature = "tbc", feature = "wotlk"))]
impl KarazhanFsm {
    pub fn new() -> Self {
        Self {
            simple: SimpleFsm::new(0),
        }
    }
}

#[cfg(any(feature = "tbc", feature = "wotlk"))]
impl Default for KarazhanFsm {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(feature = "tbc", feature = "wotlk"))]
impl EncounterFsm for KarazhanFsm {
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
