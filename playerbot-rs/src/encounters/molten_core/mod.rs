pub mod baron_geddon;
pub mod garr;
pub mod gehennas;
pub mod golemagg;
pub mod lucifron;
pub mod magmadar;
pub mod majordomo;
/// Molten Core — 10 boss encounters.
///
/// Zone ID: 2717.  40-player raid.
///
/// In addition to per-boss FSMs this module owns the instance-wide
/// `MoltenCoreFsm` wrapper, whose `phase_bt()` composes the active
/// boss's tree with a zone-wide branch. The zone-wide branch covers
/// **rune dousing** before Majordomo — any bot carrying Aqual or
/// Eternal Quintessence douses the seven pre-Majordomo runes without
/// waiting for a raid-leader command.
pub mod ragnaros;
pub mod shazzrah;
pub mod sulfuron;

use super::macros::encounter_dispatch;
use super::{EncounterEvent, EncounterFsm};
use crate::Sel;
use crate::Seq;
use crate::bot::encounter_prefs::DutyMode;
use crate::engine::bt::{BehaviorLeaf, Bt};
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use crate::ffi::ItemId;
pub use baron_geddon::BaronGeddonFsm;
pub use garr::GarrFsm;
pub use gehennas::GehennasFsm;
pub use golemagg::GolemaggFsm;
pub use lucifron::LucifronFsm;
pub use magmadar::MagmadarFsm;
pub use majordomo::MajordomoFsm;
pub use ragnaros::RagnarosFsm;
pub use shazzrah::ShazzrahFsm;
pub use sulfuron::SulfuronFsm;

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

// ── Zone-wide behavior constants ──────────────────────────────────────────

/// Sub-area IDs containing the seven pre-Majordomo runes. Sourced from the
/// Karatefylla classic build's `AreaTable`; the dousing duty is opt-in so a
/// mismatch on other builds degrades gracefully (the gate never fires).
pub const MC_RUNE_AREAS: &[u32] = &[2717];

/// `GameObject` entry ids for the seven runes that must be doused before
/// Majordomo spawns. Sourced from `gameobject_template` on the Karatefylla
/// classic build.
pub const RUNE_GO_ENTRIES: &[u32] = &[176951, 176952, 176953, 176954, 176955, 176956, 176957];

/// Aqual Quintessence — original dousing consumable.
pub const ITEM_AQUAL_QUINTESSENCE: ItemId = ItemId(17333);
/// Eternal Quintessence — later, rechargeable replacement.
pub const ITEM_ETERNAL_QUINTESSENCE: ItemId = ItemId(22754);

encounter_dispatch! {
    #[derive(Clone, PartialEq)]
    pub enum MoltenCoreBoss {
        Lucifron(LucifronFsm),
        Magmadar(MagmadarFsm),
        Gehennas(GehennasFsm),
        Garr(GarrFsm),
        BaronGeddon(BaronGeddonFsm),
        Shazzrah(ShazzrahFsm),
        Sulfuron(SulfuronFsm),
        Golemagg(GolemaggFsm),
        Majordomo(MajordomoFsm),
        Ragnaros(RagnarosFsm),
    }
}

impl TryFrom<u32> for MoltenCoreBoss {
    type Error = ();
    fn try_from(entry: u32) -> Result<Self, Self::Error> {
        match entry {
            ENTRY_LUCIFRON => Ok(Self::Lucifron(LucifronFsm::default())),
            ENTRY_MAGMADAR => Ok(Self::Magmadar(MagmadarFsm::default())),
            ENTRY_GEHENNAS => Ok(Self::Gehennas(GehennasFsm::default())),
            ENTRY_GARR => Ok(Self::Garr(GarrFsm::default())),
            ENTRY_BARON_GEDDON => Ok(Self::BaronGeddon(BaronGeddonFsm::default())),
            ENTRY_SHAZZRAH => Ok(Self::Shazzrah(ShazzrahFsm::default())),
            ENTRY_SULFURON => Ok(Self::Sulfuron(SulfuronFsm::default())),
            ENTRY_GOLEMAGG => Ok(Self::Golemagg(GolemaggFsm::default())),
            ENTRY_MAJORDOMO => Ok(Self::Majordomo(MajordomoFsm::default())),
            ENTRY_RAGNAROS => Ok(Self::Ragnaros(RagnarosFsm::default())),
            _ => Err(()),
        }
    }
}

/// Instance-wide wrapper. Forwards every trait method to the active
/// boss and composes boss BT with the zone-wide dousing branch in
/// `phase_bt()`.
pub struct MoltenCoreFsm {
    active_boss: Option<MoltenCoreBoss>,
}

impl MoltenCoreFsm {
    pub fn new() -> Self {
        Self { active_boss: None }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = MoltenCoreBoss::try_from(entry).ok();
    }

    /// Zone-wide behaviors: rune dousing before Majordomo. New
    /// branches can be added as more cross-boss mechanics are
    /// identified (e.g. Lava Run trash positioning).
    fn zone_wide_bt() -> Bt {
        Sel!(Seq!(
            Bt::InAnyArea(MC_RUNE_AREAS),
            Bt::Custom(DOUSE_ELIGIBLE),
            Bt::InCombat.not(),
            Bt::throttle(5_000, Bt::Custom(DOUSE_RUNE)),
        ))
    }
}

// ── Behavior leaves (zone-wide) ───────────────────────────────────────────

/// Duty-mode check for rune dousing. `Auto` eligibility: the bot
/// currently holds Aqual or Eternal Quintessence.
const DOUSE_ELIGIBLE: BehaviorLeaf = BehaviorLeaf {
    label: "mc_douse_eligible",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        match ctx.settings.encounter_prefs.douse_duty {
            DutyMode::Forbid => BtResult::Failure,
            DutyMode::Force => BtResult::Success,
            DutyMode::Auto => {
                if ctx.interface.has_item(ITEM_AQUAL_QUINTESSENCE)
                    || ctx.interface.has_item(ITEM_ETERNAL_QUINTESSENCE)
                {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
        }
    },
    display_text: None,
};

/// Iterate the seven rune GO entries and douse the first one in
/// range. The Quintessence item is consumed server-side on interact.
const DOUSE_RUNE: BehaviorLeaf = BehaviorLeaf {
    label: "mc_douse_rune",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        if ctx.timers.gcd_active(ctx.server_time_ms) {
            return BtResult::Failure;
        }
        for &entry in RUNE_GO_ENTRIES {
            if let Some(h) = ctx.interface.nearby_gameobject_by_entry(entry, 10.0)
                && ctx.interface.use_gameobject(h)
            {
                return BtResult::Success;
            }
        }
        BtResult::Failure
    },
    display_text: Some("Dousing Rune"),
};

impl Default for MoltenCoreFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for MoltenCoreFsm {
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
        let boss_bt = self.active_boss.as_ref().and_then(|b| b.phase_bt());
        match boss_bt {
            Some(bt) => Some(Sel!(bt, Self::zone_wide_bt())),
            None => Some(Self::zone_wide_bt()),
        }
    }
}
