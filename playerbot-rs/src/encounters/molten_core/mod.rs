pub mod baron_geddon;
pub mod garr;
pub mod lucifron;
pub mod magmadar;
/// Molten Core — 10 boss encounters.
///
/// Zone ID: 2717.  40-player raid.
pub mod ragnaros;
pub mod shazzrah;

use super::SimpleFsm;
use super::macros::encounter_dispatch;
pub use baron_geddon::BaronGeddonFsm;
pub use garr::GarrFsm;
pub use lucifron::LucifronFsm;
pub use magmadar::MagmadarFsm;
pub use ragnaros::RagnarosFsm;
pub use shazzrah::ShazzrahFsm;

// ── NPC entry IDs ─────────────────────────────────────────────────────────

pub const ENTRY_LUCIFRON: u32 = 12118;
pub const ENTRY_MAGMADAR: u32 = 11982;
pub const ENTRY_GEHENNAS: u32 = 12259;
pub const ENTRY_GARR: u32 = 12057;
pub const ENTRY_BARON_GEDDON: u32 = 12056;
pub const ENTRY_SHAZZRAH: u32 = 12264;
pub const ENTRY_SULFURON: u32 = 12098;
pub const ENTRY_GOLEMAGG: u32 = 11988;
pub const ENTRY_MAJORDOMO: u32 = 12018;
pub const ENTRY_RAGNAROS: u32 = 11502;

// Spell IDs for in-zone mechanics (shared across bosses)
pub const SPELL_FIRE_PROTECTION_POTION: crate::ffi::SpellId = crate::ffi::SpellId(17543);

encounter_dispatch! {
    #[derive(Clone, PartialEq)]
    pub enum MoltenCoreBoss {
    Generic(SimpleFsm),
    Lucifron(LucifronFsm),
    Magmadar(MagmadarFsm),
    Garr(GarrFsm),
    BaronGeddon(BaronGeddonFsm),
    Shazzrah(ShazzrahFsm),
    Ragnaros(RagnarosFsm),
    // Gehennas(SimpleFsm),
    // Sulfuron(SimpleFsm),
    // Golemagg(SimpleFsm),
    // Majordomo(SimpleFsm),
    }
}

impl TryFrom<u32> for MoltenCoreBoss {
    type Error = ();
    fn try_from(entry: u32) -> Result<Self, Self::Error> {
        match entry {
            ENTRY_RAGNAROS => Ok(Self::Ragnaros(RagnarosFsm::new())),
            ENTRY_BARON_GEDDON => Ok(Self::BaronGeddon(BaronGeddonFsm::new())),
            ENTRY_MAGMADAR => Ok(Self::Magmadar(MagmadarFsm::new())),
            ENTRY_LUCIFRON => Ok(Self::Lucifron(LucifronFsm::new())),
            ENTRY_GARR => Ok(Self::Garr(GarrFsm::new())),
            ENTRY_SHAZZRAH => Ok(Self::Shazzrah(ShazzrahFsm::new())),
            ENTRY_GEHENNAS | ENTRY_SULFURON | ENTRY_GOLEMAGG | ENTRY_MAJORDOMO => {
                Ok(Self::Generic(SimpleFsm::new(entry)))
            }
            _ => Err(()),
        }
    }
}
