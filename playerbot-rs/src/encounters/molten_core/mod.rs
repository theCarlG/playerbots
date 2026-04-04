/// Molten Core — 10 boss encounters.
///
/// Zone ID: 2717.  40-player raid.
///
/// The `MoltenCoreFsm` wraps all 10 per-boss FSMs and routes based on which
/// boss is currently the active target (detected by NPC entry ID in the
/// coordinator's boss-scan logic).

mod ragnaros;
mod baron_geddon;
mod magmadar;

use super::{EncounterEvent, EncounterFsm, SimpleFsm};
pub use ragnaros::RagnarosFsm;
pub use baron_geddon::BaronGeddonFsm;
pub use magmadar::MagmadarFsm;

// ── NPC entry IDs ─────────────────────────────────────────────────────────

pub const ENTRY_LUCIFRON:   u32 = 12118;
pub const ENTRY_MAGMADAR:   u32 = 11982;
pub const ENTRY_GEHENNAS:   u32 = 12259;
pub const ENTRY_GARR:       u32 = 12057;
pub const ENTRY_BARON_GEDDON: u32 = 12056;
pub const ENTRY_SHAZZRAH:   u32 = 12264;
pub const ENTRY_SULFURON:   u32 = 12098;
pub const ENTRY_GOLEMAGG:   u32 = 11988;
pub const ENTRY_MAJORDOMO:  u32 = 12018;
pub const ENTRY_RAGNAROS:   u32 = 11502;

// Spell IDs for in-zone mechanics (shared across bosses)
pub const SPELL_FIRE_PROTECTION_POTION: crate::ffi::SpellId = crate::ffi::SpellId(17543); // pre-fight consumable

/// Top-level FSM for Molten Core.  Delegates to per-boss FSMs.
///
/// Phase encoding:
///   0 = no active boss
///   1 = simple boss (single phase)
///   2 = Ragnaros phase 1
///   3 = Ragnaros submerged
///   4 = Ragnaros phase 2
///   10 = Baron Geddon (single phase but special mechanics)
///   11 = Magmadar (single phase but positioning mechanics)
pub struct MoltenCoreFsm {
    active_boss:  ActiveMcBoss,
    ragnaros:     RagnarosFsm,
    baron_geddon: BaronGeddonFsm,
    magmadar:     MagmadarFsm,
    simple:       SimpleFsm,
}

#[derive(Clone, Copy, PartialEq)]
enum ActiveMcBoss {
    None,
    Lucifron,
    Magmadar,
    Gehennas,
    Garr,
    BaronGeddon,
    Shazzrah,
    Sulfuron,
    Golemagg,
    Majordomo,
    Ragnaros,
}

impl MoltenCoreFsm {
    pub fn new() -> Self {
        Self {
            active_boss:  ActiveMcBoss::None,
            ragnaros:     RagnarosFsm::new(),
            baron_geddon: BaronGeddonFsm::new(),
            magmadar:     MagmadarFsm::new(),
            simple:       SimpleFsm::new(0),
        }
    }

    fn detect_boss_from_event(&mut self, event: &EncounterEvent) {
        // Boss detection could use spell_id signatures or be set from the
        // coordinator once it sees the unit's NPC entry.  For now, rely on
        // the CombatStarted event carrying context via the blackboard, or
        // detect from unique spell casts.
        // This is extended in coordinator.rs with NPC entry scanning.
        if let EncounterEvent::CombatStarted = event {
            // Will be overridden by coordinator.rs if NPC entry is known.
        }
    }
}

impl Default for MoltenCoreFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for MoltenCoreFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        self.detect_boss_from_event(event);
        match self.active_boss {
            ActiveMcBoss::Ragnaros    => self.ragnaros.update(event, boss_hp_pct, time_ms),
            ActiveMcBoss::BaronGeddon => self.baron_geddon.update(event, boss_hp_pct, time_ms),
            ActiveMcBoss::Magmadar    => self.magmadar.update(event, boss_hp_pct, time_ms),
            _ => self.simple.update(event, boss_hp_pct, time_ms),
        }
    }

    fn phase_id(&self) -> u32 {
        match self.active_boss {
            ActiveMcBoss::Ragnaros    => self.ragnaros.phase_id(),
            ActiveMcBoss::BaronGeddon => 10,
            ActiveMcBoss::Magmadar    => 11,
            ActiveMcBoss::None        => 0,
            _                         => 1,
        }
    }

    fn is_active(&self) -> bool {
        self.active_boss != ActiveMcBoss::None
    }

    fn is_done(&self) -> bool { false }

    fn boss_entry(&self) -> u32 {
        match self.active_boss {
            ActiveMcBoss::Ragnaros    => ENTRY_RAGNAROS,
            ActiveMcBoss::BaronGeddon => ENTRY_BARON_GEDDON,
            ActiveMcBoss::Magmadar    => ENTRY_MAGMADAR,
            ActiveMcBoss::Lucifron    => ENTRY_LUCIFRON,
            ActiveMcBoss::Gehennas    => ENTRY_GEHENNAS,
            ActiveMcBoss::Garr        => ENTRY_GARR,
            ActiveMcBoss::Shazzrah    => ENTRY_SHAZZRAH,
            ActiveMcBoss::Sulfuron    => ENTRY_SULFURON,
            ActiveMcBoss::Golemagg    => ENTRY_GOLEMAGG,
            ActiveMcBoss::Majordomo   => ENTRY_MAJORDOMO,
            ActiveMcBoss::None        => 0,
        }
    }
}

/// Allow the coordinator to override which boss is active (once it has NPC entry data).
impl MoltenCoreFsm {
    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = match entry {
            ENTRY_RAGNAROS    => ActiveMcBoss::Ragnaros,
            ENTRY_BARON_GEDDON => ActiveMcBoss::BaronGeddon,
            ENTRY_MAGMADAR    => ActiveMcBoss::Magmadar,
            ENTRY_LUCIFRON    => ActiveMcBoss::Lucifron,
            ENTRY_GEHENNAS    => ActiveMcBoss::Gehennas,
            ENTRY_GARR        => ActiveMcBoss::Garr,
            ENTRY_SHAZZRAH    => ActiveMcBoss::Shazzrah,
            ENTRY_SULFURON    => ActiveMcBoss::Sulfuron,
            ENTRY_GOLEMAGG    => ActiveMcBoss::Golemagg,
            ENTRY_MAJORDOMO   => ActiveMcBoss::Majordomo,
            _                 => return, // unknown entry, leave as-is
        };
    }
}
