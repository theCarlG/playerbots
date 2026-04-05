/// Blackwing Lair — 8 boss encounters.
///
/// Zone ID: 2677.  40-player raid.
///
/// Bosses in order:
///   1. Razorgore the Untamed    (entry 12435)
///   2. Vaelastrasz the Corrupt  (entry 13020)
///   3. Broodlord Lashlayer      (entry 12017)
///   4. Firemaw                  (entry 11983)
///   5. Ebonroc                  (entry 14601)
///   6. Flamegor                 (entry 14610)
///   7. Chromaggus               (entry 14020)
///   8. Nefarian                 (entry 11583)
///
/// In addition to per-boss FSMs this module owns the instance-wide
/// `BlackwingLairFsm` wrapper, whose `phase_bt()` composes the active
/// boss's tree with a zone-wide branch. The zone-wide branch currently
/// covers rogue **Suppression Device disarm duty** in the pre-Broodlord
/// corridor — a behavior that spans multiple trash pulls and no single
/// boss FSM.
pub mod broodlord;
pub mod chromaggus;
pub mod nefarian;
pub mod vaelastrasz;

use super::macros::encounter_dispatch;
use super::{EncounterEvent, EncounterFsm, SimpleFsm};
use crate::Sel;
use crate::Seq;
use crate::bot::encounter_prefs::DutyMode;
use crate::bot::state::PlayerClass;
use crate::engine::bt::{BehaviorLeaf, Bt};
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
pub use broodlord::BroodlordFsm;
pub use chromaggus::ChromaggusFsm;
pub use nefarian::NefarianFsm;
pub use vaelastrasz::VaelastraszFsm;

// ── NPC entry IDs ─────────────────────────────────────────────────────────

pub const ENTRY_RAZORGORE: u32 = 12435;
pub const ENTRY_VAELASTRASZ: u32 = 13020;
pub const ENTRY_BROODLORD: u32 = 12017;
pub const ENTRY_FIREMAW: u32 = 11983;
pub const ENTRY_EBONROC: u32 = 14601;
pub const ENTRY_FLAMEGOR: u32 = 14610;
pub const ENTRY_CHROMAGGUS: u32 = 14020;
pub const ENTRY_NEFARIAN: u32 = 11583;

// ── Zone-wide behavior constants ──────────────────────────────────────────

/// Sub-area id for the Suppression Room corridor leading to Broodlord.
/// TODO: confirm against `AreaTable` DBC for the target build. If
/// wrong, the gate silently never fires — the behavior is opt-in.
pub const AREA_SUPPRESSION_ROOM: u32 = 2802;

/// `GameObject` entry id for the Suppression Device trap in the
/// pre-Broodlord corridor.
pub const GO_SUPPRESSION_DEVICE: u32 = 179784;

encounter_dispatch! {
    #[derive(Clone, PartialEq)]
    pub enum BlackwingLairBoss {
        Razorgore(SimpleFsm),
        Vaelastrasz(VaelastraszFsm),
        Broodlord(BroodlordFsm),
        Firemaw(SimpleFsm),
        Ebonroc(SimpleFsm),
        Flamegor(SimpleFsm),
        Chromaggus(ChromaggusFsm),
        Nefarian(NefarianFsm),
    }
}

impl TryFrom<u32> for BlackwingLairBoss {
    type Error = ();
    fn try_from(entry: u32) -> Result<Self, Self::Error> {
        match entry {
            ENTRY_RAZORGORE => Ok(Self::Razorgore(SimpleFsm::new(entry))),
            ENTRY_VAELASTRASZ => Ok(Self::Vaelastrasz(VaelastraszFsm::default())),
            ENTRY_BROODLORD => Ok(Self::Broodlord(BroodlordFsm::default())),
            ENTRY_FIREMAW => Ok(Self::Firemaw(SimpleFsm::new(entry))),
            ENTRY_EBONROC => Ok(Self::Ebonroc(SimpleFsm::new(entry))),
            ENTRY_FLAMEGOR => Ok(Self::Flamegor(SimpleFsm::new(entry))),
            ENTRY_CHROMAGGUS => Ok(Self::Chromaggus(ChromaggusFsm::default())),
            ENTRY_NEFARIAN => Ok(Self::Nefarian(NefarianFsm::default())),
            _ => Err(()),
        }
    }
}

/// Instance-wide wrapper. Forwards every trait method to the active
/// boss, composes boss BT with the zone-wide branch in `phase_bt()`.
pub struct BlackwingLairFsm {
    active_boss: Option<BlackwingLairBoss>,
}

impl BlackwingLairFsm {
    pub fn new() -> Self {
        Self { active_boss: None }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = BlackwingLairBoss::try_from(entry).ok();
    }

    /// Behaviors that span the entire instance with no active boss —
    /// class/area/duty-gated maintenance work. Currently: Suppression
    /// Device disarm duty. New branches are added as more zone-wide
    /// mechanics are identified.
    fn zone_wide_bt() -> Bt {
        Sel!(Seq!(
            Bt::InArea(AREA_SUPPRESSION_ROOM),
            Bt::Custom(SUPPRESSION_ELIGIBLE),
            Bt::InCombat.not(),
            Bt::throttle(3_000, Bt::Custom(DISARM_DEVICE)),
        ))
    }
}

// ── Behavior leaves (zone-wide) ───────────────────────────────────────────

/// Duty-mode check for Suppression Device disarm. `Force` / `Forbid`
/// always win; `Auto` falls back to "any rogue participates".
const SUPPRESSION_ELIGIBLE: BehaviorLeaf = BehaviorLeaf {
    label: "bwl_suppression_eligible",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        match ctx.settings.encounter_prefs.suppression_duty {
            DutyMode::Forbid => BtResult::Failure,
            DutyMode::Force => BtResult::Success,
            DutyMode::Auto => {
                if ctx.class == PlayerClass::Rogue {
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }
        }
    },
    display_text: None,
};

/// Locate and disarm the nearest Suppression Device. Gated on GCD to
/// stay consistent with other action leaves.
const DISARM_DEVICE: BehaviorLeaf = BehaviorLeaf {
    label: "bwl_disarm_device",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        if ctx.timers.gcd_active(ctx.server_time_ms) {
            return BtResult::Failure;
        }
        match ctx
            .interface
            .nearby_gameobject_by_entry(GO_SUPPRESSION_DEVICE, 15.0)
        {
            Some(h) if ctx.interface.use_gameobject(h) => BtResult::Success,
            _ => BtResult::Failure,
        }
    },
    display_text: Some("Disarming Suppression Device"),
};

impl Default for BlackwingLairFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for BlackwingLairFsm {
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

// ── Zone-wide tick handlers ───────────────────────────────────────────────
