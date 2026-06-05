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
use cmangos::ItemId;
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

// Trash: Flamewaker Priest/Healer packs heal each other and the giants —
// their casts must be interrupted or pulls drag on forever.
pub const ENTRY_FLAMEWAKER_PRIEST: u32 = 11662;
pub const ENTRY_FLAMEWAKER_HEALER: u32 = 11663;
// Core Hounds cleave and revive each other — the tank drags them away from
// the raid. Lava Surgers charge — the raid stacks so the charge/cleave is
// shared instead of scattering people into other packs.
pub const ENTRY_CORE_HOUND: u32 = 11671;
pub const ENTRY_ANCIENT_CORE_HOUND: u32 = 11673;
pub const ENTRY_LAVA_SURGER: u32 = 12101;

// Spell IDs for in-zone mechanics (shared across bosses)
pub const SPELL_FIRE_PROTECTION_POTION: cmangos::SpellId = cmangos::SpellId(17543);

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
        Sel!(
            // Trash: interrupt a nearby Flamewaker Priest/Healer's cast so the
            // packs don't heal through the raid's damage.
            Bt::throttle(1_000, Bt::Custom(INTERRUPT_FLAMEWAKER)),
            // Trash: the tank drags Core Hounds away from the raid (they
            // cleave and revive each other if killed apart).
            Seq!(
                Bt::InCombat,
                Bt::IsTank,
                Bt::Custom(TARGET_IS_CORE_HOUND),
                Bt::MoveAwayFromRaid(20.0),
            ),
            // Trash: non-tanks stack on the tank when a Lava Surger is
            // charging, so the charge + cleave hit a grouped raid.
            Seq!(
                Bt::InCombat,
                Bt::IsTank.not(),
                Bt::Custom(LAVA_SURGER_NEARBY),
                Bt::Custom(STACK_ON_TANK),
            ),
            // Rune dousing before Majordomo.
            Seq!(
                Bt::InAnyArea(MC_RUNE_AREAS),
                Bt::Custom(DOUSE_ELIGIBLE),
                Bt::InCombat.not(),
                Bt::throttle(5_000, Bt::Custom(DOUSE_RUNE)),
            ),
        )
    }
}

/// True when the bot's current target is a Core Hound (the tank drags these
/// away from the raid).
const TARGET_IS_CORE_HOUND: BehaviorLeaf = BehaviorLeaf {
    label: "mc_target_is_core_hound",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        match ctx.current_target() {
            Some(t) => {
                let e = ctx.interface.get_unit_snapshot(t).npc_entry;
                if e == ENTRY_CORE_HOUND || e == ENTRY_ANCIENT_CORE_HOUND {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
            None => BtResult::Failure,
        }
    },
    display_text: None,
};

/// True when a Lava Surger (the charging elemental) is nearby.
const LAVA_SURGER_NEARBY: BehaviorLeaf = BehaviorLeaf {
    label: "mc_lava_surger_nearby",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        let units = ctx.interface.get_nearby_units(30.0, true);
        for &u in units.iter() {
            if ctx.interface.get_unit_snapshot(u).npc_entry == ENTRY_LAVA_SURGER {
                return BtResult::Success;
            }
        }
        BtResult::Failure
    },
    display_text: None,
};

/// Move to within ~5y of the main tank (stack up). `Running` while moving,
/// `Success` once stacked, `Failure` if there's no known tank position.
const STACK_ON_TANK: BehaviorLeaf = BehaviorLeaf {
    label: "mc_stack_on_tank",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        let Some(tank) = ctx.group_tank() else {
            return BtResult::Failure;
        };
        let Some(pos) = ctx.interface.get_player_position(tank) else {
            return BtResult::Failure;
        };
        let me = ctx.snap.self_.pos;
        if (me.x - pos.x).powi(2) + (me.y - pos.y).powi(2) <= 5.0 * 5.0 {
            return BtResult::Success; // already stacked
        }
        if ctx.interface.move_to(pos.x, pos.y, pos.z) {
            BtResult::Running
        } else {
            BtResult::Failure
        }
    },
    display_text: Some("Stacking on tank"),
};

// ── Behavior leaves (trash) ───────────────────────────────────────────────

/// Interrupt a nearby casting Flamewaker Priest/Healer (the Molten Core trash
/// healers). Scans nearby hostiles for one mid-cast and fires the bot's
/// class interrupt at it — an off-target interrupt the reactive
/// (current-target) interrupt can't do.
const INTERRUPT_FLAMEWAKER: BehaviorLeaf = BehaviorLeaf {
    label: "mc_interrupt_flamewaker",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        if ctx.timers.gcd_active(ctx.server_time_ms) {
            return BtResult::Failure;
        }
        let spells = crate::engine::bt::class_interrupt_spells(ctx.class);
        if spells.is_empty() {
            return BtResult::Failure;
        }
        let units = ctx.interface.get_nearby_units(30.0, true);
        for &u in units.iter() {
            let snap = ctx.interface.get_unit_snapshot(u);
            if !snap.is_casting
                || (snap.npc_entry != ENTRY_FLAMEWAKER_PRIEST
                    && snap.npc_entry != ENTRY_FLAMEWAKER_HEALER)
                || !ctx.interface.is_casting_interruptible(u)
            {
                continue;
            }
            for &spell in spells {
                if ctx.interface.can_cast(spell, u) && ctx.interface.cast_spell(spell, u) {
                    ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
                    return BtResult::Success;
                }
            }
        }
        BtResult::Failure
    },
    display_text: Some("Interrupting Flamewaker"),
};

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
/// range that no other bot has already claimed. The Quintessence item
/// is consumed server-side on interact. Each rune entry is unique within
/// the instance, so the entry id is sufficient as a `DouseRune` claim
/// subject — see `engine::claim::ClaimData::DouseRune`.
///
/// Claim TTL is short (~3 s) so a dying claimant releases the rune
/// quickly to the next eligible bot.
const DOUSE_RUNE: BehaviorLeaf = BehaviorLeaf {
    label: "mc_douse_rune",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        if ctx.timers.gcd_active(ctx.server_time_ms) {
            return BtResult::Failure;
        }
        const DOUSE_CLAIM_TTL_MS: u64 = 3_000;
        for &entry in RUNE_GO_ENTRIES {
            // Skip runes another bot already owns. Cheap read — no lock churn.
            if ctx.is_douse_claimed_by_other(entry) {
                continue;
            }
            let Some(h) = ctx.interface.nearby_gameobject_by_entry(entry, 10.0) else {
                continue;
            };
            // Claim before acting so a concurrent doser races us at the
            // table, not at the GameObject. If we lose the claim race,
            // try the next rune.
            if !ctx.try_claim_douse(entry, DOUSE_CLAIM_TTL_MS) {
                continue;
            }
            if ctx.interface.use_gameobject(h) {
                // Success — keep the claim alive for its TTL so a fast
                // double-tick by another bot doesn't re-douse this rune.
                return BtResult::Success;
            }
            // Use failed (out of range, busy, etc.) — release immediately
            // so another bot can pick it up next tick.
            ctx.release_douse(entry);
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
    fn set_boss_entry(&mut self, entry: u32) {
        // Only switch if we don't already have this boss active.
        let dominated = self
            .active_boss
            .as_ref()
            .is_some_and(|b| b.boss_entry() == entry);
        if !dominated {
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
        let boss_bt = self.active_boss.as_ref().and_then(|b| b.phase_bt(fsm));
        match boss_bt {
            Some(bt) => Some(Sel!(bt, Self::zone_wide_bt())),
            None => Some(Self::zone_wide_bt()),
        }
    }
}
