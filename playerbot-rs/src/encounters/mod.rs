pub mod aq20;
pub mod aq40;
pub mod blackwing_lair;
/// Encounter FSMs and boss mechanics.
///
/// Each sub-module implements one raid/dungeon encounter.
/// Encounters are gated by expansion feature flags where needed.
///
/// # Integration
/// `BotState::encounter` holds the active `Box<dyn EncounterFsm>`.
/// The encounter is activated via `coordinator::encounter_for_zone(zone_id)`.
/// Push events (from tick.rs) are dispatched to the FSM via `EncounterFsm::update()`.
/// The BT reads `encounter.phase_id()` to select phase-appropriate subtrees.
pub mod bt;
pub mod bt_overrides;
pub mod coordinator;
pub mod macros;

// Raids
pub mod molten_core;
pub mod naxxramas;
pub mod onyxias_lair;
pub mod zul_gurub;

// TBC content
#[cfg(any(feature = "tbc", feature = "wotlk"))]
pub mod karazhan;

// Battlegrounds
pub mod bg;

use crate::engine::bt::Bt;
use crate::ffi::SpellId;

/// Reserved phase IDs shared by all encounters.
///
/// Encounters are free to use any `u32` for their internal phases, but these
/// two values are conventional slots that every FSM gets for free:
///
/// - `PHASE_PREPULL`: the FSM is constructed but `CombatStarted` has not fired.
/// - `PHASE_VICTORY`: `is_done()` is true (boss dead). The FSM may still
///   return a `phase_bt()` for a brief victory routine before the encounter
///   is torn down.
pub const PHASE_PREPULL: u32 = 0;
pub const PHASE_VICTORY: u32 = u32::MAX;

/// Trait all encounter FSMs implement.
///
/// Each boss is a state machine. Each state can have an associated BT that
/// overrides the normal combat rotation (via `phase_bt()`). When `phase_bt()`
/// returns `None`, the normal rotation runs.
pub trait EncounterFsm: Send {
    /// Update the FSM from a push event and current boss HP.
    ///
    /// Called once per tick before the BT runs, from `process_events` in tick.rs.
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, server_time_ms: u64);

    /// Current phase ID as a `u32` (interpretation is per-encounter).
    ///
    /// Phase 0 means pre-pull / not yet active.
    fn phase_id(&self) -> u32;

    /// True once the encounter has been pulled (boss engaged).
    fn is_active(&self) -> bool;

    /// True once the encounter is finished (boss dead or reset).
    fn is_done(&self) -> bool;

    /// NPC entry ID of the primary boss tracked by this FSM.
    /// Used by the coordinator to match the right FSM to a given target.
    fn boss_entry(&self) -> u32;

    /// Activate the sub-FSM for a specific boss by NPC entry ID.
    ///
    /// Instance wrappers (MC, BWL, etc.) use this to switch to the
    /// correct boss FSM when the bot's target matches a known boss.
    /// Default: no-op (single-boss or simple FSMs ignore this).
    fn set_boss_entry(&mut self, _entry: u32) {}

    /// Hint for Heigan-style safe zone positioning (1-4). 0 = not applicable.
    fn safe_zone_hint(&self) -> u8 {
        0
    }

    /// Returns the BT override for the current phase, if any.
    ///
    /// When `Some`, this BT runs instead of the normal combat rotation.
    /// When `None`, the normal rotation runs unmodified.
    ///
    /// BTs are built once at FSM construction and stored in the boss struct.
    /// Returns a reference to the pre-built tree — zero allocation per tick,
    /// no vtable dispatch (concrete `Bt` data type).
    ///
    /// Transitions (pull, phase change, victory) are expressed as dedicated
    /// phase states inside each FSM's phase enum; there is no separate edge
    /// hook. The FSM's `update()` flips into the transition phase and
    /// `phase_bt()` returns the matching tree.
    /// The BT itself must be stateless — per-bot state (e.g. throttle
    /// timestamps) lives on `BotState`, not in the tree. Beyond that, an
    /// encounter FSM may either own a BT as a struct field (per-instance) or
    /// return a shared `&'static Bt` built once via `OnceLock` — both are
    /// valid under the elided lifetime below.
    ///
    /// # Instance-wide (cross-boss) behaviors
    ///
    /// Instance wrappers (e.g. `BlackwingLairFsm`, `MoltenCoreFsm`) are
    /// free to return a composed
    /// `Sel!(active_boss_bt, zone_wide_bt())` so cross-boss behaviors
    /// (class-gated trash maintenance, sub-area rituals like BWL's
    /// Suppression Device disarm duty or MC's rune dousing) can fire
    /// when no boss is active, without hijacking boss-BT priority when
    /// one is pulled. The `zone_wide_bt()` helper is a free-standing
    /// `fn` defined inline in the instance's `mod.rs`; instance
    /// specific gameplay logic lives in that module rather than in
    /// `engine/bt.rs`, and is reached from the tree via
    /// [`Bt::Custom`](crate::engine::bt::Bt::Custom) leaves.
    fn phase_bt(&self) -> Option<Bt> {
        None
    }
}

/// Events that can drive FSM transitions.
#[derive(Debug, Clone)]
pub enum EncounterEvent {
    /// Regular tick — no specific event.
    None,
    /// A unit died (could be boss, player, or add).
    UnitDied { victim: u64 },
    /// A unit started or completed a spell cast.
    SpellCast {
        caster: u64,
        spell_id: SpellId,
        success: bool,
    },
    /// An aura was applied to or removed from a unit.
    AuraChanged {
        unit: u64,
        spell_id: SpellId,
        applied: bool,
    },
    /// This bot was pulled into combat.
    CombatStarted,
    /// The group wiped (all players dead / boss reset).
    GroupWipe,
}

/// A trivial single-phase FSM for bosses with no phase transitions.
/// Used as the default for mechanically simple encounters.
#[derive(PartialEq, Clone)]
pub struct SimpleFsm {
    entry: u32,
    active: bool,
    done: bool,
}

impl SimpleFsm {
    pub fn new(entry: u32) -> Self {
        Self {
            entry,
            active: false,
            done: false,
        }
    }
}

impl EncounterFsm for SimpleFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { victim: _ } => {
                // If the boss died, mark done.
                // (Proper detection requires NPC entry matching — done in coordinator.)
            }
            EncounterEvent::GroupWipe => {
                self.active = false;
                self.done = true;
            }
            _ => {}
        }
    }
    fn phase_id(&self) -> u32 {
        if self.active { 1 } else { 0 }
    }
    fn is_active(&self) -> bool {
        self.active
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        self.entry
    }
}

/// Generic instance wrapper for raids/dungeons whose bosses are driven
/// by a dispatch enum (`T: EncounterFsm + TryFrom<u32>`) and which have
/// no zone-wide, cross-boss behaviors to layer on top.
///
/// Bespoke instance FSMs (`BlackwingLairFsm`, `MoltenCoreFsm`) are used
/// where `phase_bt()` needs to compose the active boss tree with an
/// instance-wide branch (suppression disarm, rune dousing, …). For
/// simpler instances — regular 5-mans where at most one boss has a
/// scripted FSM and the rest are `SimpleFsm` — this wrapper is the
/// lightest-weight home.
pub struct BasicInstanceFsm<T>
where
    T: EncounterFsm + TryFrom<u32>,
{
    active_boss: Option<T>,
}

impl<T> BasicInstanceFsm<T>
where
    T: EncounterFsm + TryFrom<u32>,
{
    pub fn new() -> Self {
        Self { active_boss: None }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = T::try_from(entry).ok();
    }
}

impl<T> Default for BasicInstanceFsm<T>
where
    T: EncounterFsm + TryFrom<u32>,
{
    fn default() -> Self {
        Self { active_boss: None }
    }
}

impl<T> EncounterFsm for BasicInstanceFsm<T>
where
    T: EncounterFsm + TryFrom<u32>,
{
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

    fn phase_bt(&self) -> Option<Bt> {
        self.active_boss.as_ref().and_then(|b| b.phase_bt())
    }
}
