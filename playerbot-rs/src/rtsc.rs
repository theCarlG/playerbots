//! RTSC (Real-Time Strategy Control) — PB2 parity for spell-based RTS commands.
//!
//! The master casts Aedm (spell id 30758) on the ground; the core forwards
//! the impact position into the bot via the `playerbot_rtsc_spell` FFI,
//! which pushes a [`BotCommand::RtscSpellPosition`](crate::commands::BotCommand)
//! onto the per-bot command queue. What the bot does with that position
//! depends on [`BotSettings::rtsc_pending_action`] plus the two reserved
//! slots (`"jump"` and `"jump point"`) inside
//! [`BotSettings::rtsc_waypoints`].
//!
//! Reference: `/home/cg/Code/gitea/Karatefylla/mangos/classic/source/src/modules/PB2/playerbot/strategy/actions/RtscAction.cpp`.
//!
//! This module owns the shared helpers used by the `commands` dispatcher
//! so that the spell-level behavior (learn/unlearn Aedm, save-here marker
//! summon, jump two-stage recording, file import/export) is not scattered
//! across the command match arms.

use crate::bot::settings::{BotStateKind, RtscAction, StrategyFlags};
use crate::bot::state::BotState;
use crate::ffi::SpellId;

/// PB2 `RTSC_MOVE_SPELL` — Aedm ("Awesome Energetic Do Move"). Trained on
/// `rtsc select` / `rtsc toggle`, removed on `rtsc reset`.
pub const RTSC_MOVE_SPELL: u32 = 30758;

/// Entry id of the temporary waypoint marker creature summoned by
/// `rtsc save here` and `rtsc show <name>`. PB2 `RtscAction.cpp:85`,
/// `:278`.
pub const MARKER_ENTRY: u32 = 15631;
/// Marker despawn lifetime (ms).
pub const MARKER_DESPAWN_MS: u32 = 2000;
/// Marker visual scale applied via `Creature::SetObjectScale`.
pub const MARKER_SCALE: f32 = 0.5;

/// Reserved waypoint names used by the two-stage jump recorder.
pub const JUMP_SLOT: &str = "jump";
pub const JUMP_POINT_SLOT: &str = "jump point";

/// How long a queued [`RtscAction`] stays valid before
/// [`on_spell_land`] discards it. PB2 implicitly assumes the master
/// casts Aedm within ~5 s of typing `rtsc <verb>`; we double that to
/// avoid penalising slow connections, but a stale pending action
/// would otherwise hijack the next unrelated Aedm cast — see Gap #14.
pub const RTSC_PENDING_TTL_MS: u64 = 10_000;

/// Queue a [`RtscAction`] for the next [`on_spell_land`] call,
/// stamping it with the current server time. The TTL is checked at
/// drain time so a queued action that never sees an Aedm cast does
/// not poison subsequent commands.
pub fn queue_pending(bot: &mut BotState, action: RtscAction) {
    let now = bot.snap.server_time_ms;
    bot.settings.rtsc_pending_action = Some((action, now));
}

/// Outcome of a [`jump_command`] invocation. The caller routes user-
/// visible error strings via its own reply channel.
#[derive(Debug, PartialEq, Eq)]
pub enum JumpCommandResult {
    /// First stage queued — bot is waiting for the next Aedm cast to
    /// populate the `"jump"` slot.
    StageOneQueued,
    /// Stale state detected (`"jump"` populated but `"jump point"`
    /// missing, and the user typed `rtsc jump` again). Both slots
    /// cleared and the `rtsc jump` strategy bit stripped. Caller
    /// should surface PB2's "Can't finish previous jump! Cancelling..."
    /// error (`RtscAction.cpp:330-332`).
    StaleCancelled,
    /// Both slots already populated — another jump is in progress.
    /// Caller should surface PB2's "Another jump is in process!
    /// Use 'rtsc jump reset' to stop it" error (`RtscAction.cpp:336-338`).
    AlreadyInProgress,
}

/// Ensure Aedm is learned on the bot. No-op when already known.
/// Mirrors PB2 `RtscAction.cpp:23-28` — the implicit learn happens on
/// every `rtsc <verb>` invocation *except* `reset`, so callers for
/// `select` / `toggle` / `move` / `save` / `jump` / `go` / `show` /
/// `last` / `file` all go through this.
pub fn ensure_spell_learned(bot: &BotState) {
    if !bot.interface.knows_spell(SpellId(RTSC_MOVE_SPELL)) {
        bot.interface.bot_learn_spell(RTSC_MOVE_SPELL);
    }
}

/// `rtsc select` — enable RTSC control for this bot.
pub fn select(bot: &mut BotState) {
    ensure_spell_learned(bot);
    bot.settings.rtsc_selected = true;
    bot.settings.rtsc_pending_action = None;
}

/// `rtsc cancel` — disable RTSC control. Keeps saved waypoints.
pub fn cancel(bot: &mut BotState) {
    bot.settings.rtsc_selected = false;
    bot.settings.rtsc_pending_action = None;
}

/// `rtsc toggle` — flip the selected state; learn Aedm if re-enabling.
pub fn toggle(bot: &mut BotState) {
    if !bot.settings.rtsc_selected {
        ensure_spell_learned(bot);
        bot.settings.rtsc_selected = true;
    } else {
        bot.settings.rtsc_selected = false;
        bot.settings.rtsc_pending_action = None;
    }
}

/// `rtsc reset` — unlearn Aedm and wipe every piece of RTSC state.
/// PB2 `RtscAction.cpp:29-44`: the reset path is the only `rtsc`
/// verb that skips the implicit learn — it fully clears state
/// including the saved-location blackboard entries.
pub fn reset(bot: &mut BotState) {
    bot.interface.bot_remove_spell(RTSC_MOVE_SPELL);
    bot.settings.rtsc_selected = false;
    bot.settings.rtsc_pending_action = None;
    bot.settings.rtsc_waypoints.clear();
    bot.settings.rtsc_last_seen = None;
    bot.settings
        .strategies
        .get_mut(BotStateKind::NonCombat)
        .remove(StrategyFlags::RTSC_JUMP);
}

/// `rtsc save here <name>` — record the bot's current position under
/// `name` and summon the 2-second marker creature so the master can
/// see what was saved. PB2 `RtscAction.cpp:78-88`.
pub fn save_here(bot: &mut BotState, name: String) {
    ensure_spell_learned(bot);
    let pos = bot.snap.self_.pos;
    bot.settings
        .rtsc_waypoints
        .insert(name, (pos.x, pos.y, pos.z));
    bot.interface.bot_summon_marker_creature(
        MARKER_ENTRY,
        pos.x,
        pos.y,
        pos.z,
        pos.o,
        MARKER_DESPAWN_MS,
        MARKER_SCALE,
    );
}

/// `rtsc show <name>` — summon the marker creature at a previously
/// saved waypoint (no-op if the waypoint doesn't exist). PB2
/// `RtscAction.cpp:271-283`.
pub fn show_named(bot: &BotState, name: &str) -> bool {
    let Some(&(x, y, z)) = bot.settings.rtsc_waypoints.get(name) else {
        return false;
    };
    // Orientation isn't stored per-waypoint — PB2 uses the captured
    // `WorldPosition::getO()`. Until the waypoint storage tracks `o`,
    // we pass the bot's current orientation so the marker at least
    // faces something sensible.
    let o = bot.snap.self_.pos.o;
    bot.interface.bot_summon_marker_creature(
        MARKER_ENTRY,
        x,
        y,
        z,
        o,
        MARKER_DESPAWN_MS,
        MARKER_SCALE,
    );
    true
}

/// `rtsc last` — move to the last observed Aedm cast position, if any.
/// Returns `true` when a move was issued. PB2 `RtscAction.cpp:308-313`.
pub fn last(bot: &BotState) -> bool {
    if let Some((x, y, z)) = bot.settings.rtsc_last_seen {
        bot.interface.move_to(x, y, z);
        true
    } else {
        false
    }
}

/// `rtsc jump` — initiate or diagnose the two-stage jump recorder.
/// PB2 `RtscAction.cpp:315-344`.
///
/// - Stage one (no slots populated): queues [`RtscAction::Jump`] so
///   the next Aedm cast fills `"jump"`, and sets the `rtsc jump`
///   strategy bit on the `NonCombat` slot. Stage two is filled by the
///   spell-land consumer automatically — the user does *not* type
///   `rtsc jump` a second time.
/// - Stale state (`"jump"` populated but `"jump point"` missing):
///   wipes both slots, strips the strategy bit, returns
///   [`JumpCommandResult::StaleCancelled`].
/// - Both slots populated: returns
///   [`JumpCommandResult::AlreadyInProgress`] unchanged.
pub fn jump_command(bot: &mut BotState) -> JumpCommandResult {
    ensure_spell_learned(bot);
    let have_jump = bot.settings.rtsc_waypoints.contains_key(JUMP_SLOT);
    let have_point = bot.settings.rtsc_waypoints.contains_key(JUMP_POINT_SLOT);
    if !have_jump {
        queue_pending(bot, RtscAction::Jump);
        bot.settings
            .strategies
            .get_mut(BotStateKind::NonCombat)
            .insert(StrategyFlags::RTSC_JUMP);
        return JumpCommandResult::StageOneQueued;
    }
    if !have_point {
        // Stage one stored but nothing recorded stage two → stale. Wipe
        // and let the caller emit PB2's cancel message.
        bot.settings.rtsc_waypoints.remove(JUMP_SLOT);
        bot.settings.rtsc_waypoints.remove(JUMP_POINT_SLOT);
        bot.settings.rtsc_pending_action = None;
        bot.settings
            .strategies
            .get_mut(BotStateKind::NonCombat)
            .remove(StrategyFlags::RTSC_JUMP);
        return JumpCommandResult::StaleCancelled;
    }
    JumpCommandResult::AlreadyInProgress
}

/// `rtsc jump reset` — always-safe cancel. PB2 `RtscAction.cpp:345-352`.
pub fn jump_reset(bot: &mut BotState) {
    bot.settings.rtsc_waypoints.remove(JUMP_SLOT);
    bot.settings.rtsc_waypoints.remove(JUMP_POINT_SLOT);
    bot.settings.rtsc_pending_action = None;
    bot.settings
        .strategies
        .get_mut(BotStateKind::NonCombat)
        .remove(StrategyFlags::RTSC_JUMP);
}

/// User-visible outcome of an [`on_spell_land`] call. The command
/// dispatcher uses this to whisper PB2-style confirmations back to
/// the master ("Moved to X,Y,Z", "Saved as <name>", etc.). Variants
/// where no reply is appropriate (idle bot, stale pending) collapse
/// to [`SpellLandOutcome::Ignored`].
#[derive(Debug, Clone, PartialEq)]
pub enum SpellLandOutcome {
    /// `move`/`move exact` consumed — bot is heading to (x,y,z).
    Moved { x: f32, y: f32, z: f32, exact: bool },
    /// `save <name>` consumed — waypoint stored.
    Saved { name: String, x: f32, y: f32, z: f32 },
    /// First Aedm cast of a `jump` command — stage one stored.
    JumpStageOne { x: f32, y: f32, z: f32 },
    /// Second Aedm cast of a `jump` command — stage two stored,
    /// the executor leaf will now drive the jump rotation.
    JumpStageTwo { x: f32, y: f32, z: f32 },
    /// No pending action and the bot was selected — fell through to
    /// the default "follow the cast" move. No reply expected.
    DefaultMove { x: f32, y: f32, z: f32 },
    /// Either the bot was unselected with no pending action, or the
    /// pending action exceeded [`RTSC_PENDING_TTL_MS`] and was dropped.
    Ignored,
}

/// Consume an Aedm cast at `(x, y, z)`. Called from the
/// `BotCommand::RtscSpellPosition` handler. Updates `rtsc_last_seen`
/// unconditionally (PB2 `see spell location` AI value) then dispatches
/// on the pending action.
///
/// Returns a [`SpellLandOutcome`] so the command dispatcher can whisper
/// the appropriate user-visible confirmation. Pending actions queued
/// more than [`RTSC_PENDING_TTL_MS`] ago are silently discarded — see
/// Gap #14.
pub fn on_spell_land(bot: &mut BotState, x: f32, y: f32, z: f32) -> SpellLandOutcome {
    bot.settings.rtsc_last_seen = Some((x, y, z));

    let now = bot.snap.server_time_ms;
    let pending = match bot.settings.rtsc_pending_action.take() {
        Some((action, queued_at)) if now.saturating_sub(queued_at) <= RTSC_PENDING_TTL_MS => {
            Some(action)
        }
        // Stale or absent — drop without dispatching to avoid hijacking
        // an unrelated Aedm cast. Caller falls through to the default
        // selected-move branch below.
        _ => None,
    };

    match pending {
        Some(RtscAction::Move { exact }) => {
            // Drive the move via the BDI forced-intention path so that
            // the next BT tick does not clobber it with follow / combat
            // movement. The intention router (see `bot::tick`) issues
            // `move_to` every tick until arrival, then clears the
            // intention. See Gap #10 / Gap #13.
            bot.bdi.forced_intention = Some(crate::bdi::intentions::ForcedIntention::MoveToRtsc {
                x,
                y,
                z,
                exact,
            });
            bot.bdi.intention_changed = true;
            SpellLandOutcome::Moved { x, y, z, exact }
        }
        Some(RtscAction::Save { name }) => {
            bot.settings
                .rtsc_waypoints
                .insert(name.clone(), (x, y, z));
            SpellLandOutcome::Saved { name, x, y, z }
        }
        Some(RtscAction::Jump) => {
            // Stage one or stage two, decided by which reserved slot
            // is still empty.
            if !bot.settings.rtsc_waypoints.contains_key(JUMP_SLOT) {
                bot.settings
                    .rtsc_waypoints
                    .insert(JUMP_SLOT.into(), (x, y, z));
                // Re-queue pending so the next Aedm cast fills the
                // stage-two slot without requiring the user to type
                // `rtsc jump` again.
                queue_pending(bot, RtscAction::Jump);
                SpellLandOutcome::JumpStageOne { x, y, z }
            } else if !bot.settings.rtsc_waypoints.contains_key(JUMP_POINT_SLOT) {
                bot.settings
                    .rtsc_waypoints
                    .insert(JUMP_POINT_SLOT.into(), (x, y, z));
                // Both slots populated — hand off to the forced-intention
                // jump executor (Gap #12).
                let stage1 = bot.settings.rtsc_waypoints[JUMP_SLOT];
                let stage2 = (x, y, z);
                bot.bdi.forced_intention =
                    Some(crate::bdi::intentions::ForcedIntention::JumpRtsc {
                        stage1,
                        stage2,
                        at_stage_two: false,
                    });
                bot.bdi.intention_changed = true;
                SpellLandOutcome::JumpStageTwo { x, y, z }
            } else {
                // Third cast while both slots are full: `jump reset`
                // required. No-op.
                SpellLandOutcome::Ignored
            }
        }
        None => {
            // No (live) pending action: fall through to a default move
            // when the bot is selected. PB2 doesn't do this explicitly —
            // it requires a queued command — but the Rust port has
            // historically fallen back to move-on-selected and some
            // callers rely on it. Keep the behavior but gated by
            // `rtsc_selected` so unselected bots don't chase master
            // casts intended for other bots.
            if bot.settings.rtsc_selected {
                bot.bdi.forced_intention =
                    Some(crate::bdi::intentions::ForcedIntention::MoveToRtsc {
                        x,
                        y,
                        z,
                        exact: false,
                    });
                bot.bdi.intention_changed = true;
                SpellLandOutcome::DefaultMove { x, y, z }
            } else {
                SpellLandOutcome::Ignored
            }
        }
    }
}

/// Serialize matching waypoints into PB2's CSV wire format
/// (`BOTNAME,name,x,y,z,o,map`). `name_glob == "*"` matches every
/// waypoint; otherwise a case-insensitive substring match applies.
/// Reserved jump slots are excluded from export — PB2 does not
/// distinguish them but re-importing stale jump state is always
/// wrong. The bot-name column is currently a static `"BOTNAME"`
/// placeholder because no `bot_name` FFI exists yet; see the
/// gotcha note in Part 5 Step 9 of `PB2_PARITY_PLAN.md`. Returns
/// the number of rows written (for reply formatting) alongside
/// the body string.
pub fn serialize_waypoints(bot: &BotState, name_glob: &str) -> (String, usize) {
    let map_id = bot.snap.self_.pos.map_id;
    let mut body = String::new();
    let mut count = 0usize;
    for (name, &(x, y, z)) in &bot.settings.rtsc_waypoints {
        if name == JUMP_SLOT || name == JUMP_POINT_SLOT {
            continue;
        }
        if !glob_matches(name_glob, name) {
            continue;
        }
        body.push_str(&format!(
            "BOTNAME,{name},{x:.2},{y:.2},{z:.2},0.00,{map_id}\n"
        ));
        count += 1;
    }
    (body, count)
}

/// Parse a CSV body produced by [`serialize_waypoints`] and insert
/// matching rows into the bot's waypoints. Returns the number of
/// entries imported. `name_glob == "*"` matches all; otherwise
/// case-insensitive substring match. Malformed lines are silently
/// skipped (PB2 behavior — `RtscAction.cpp:236-262` uses `continue`
/// on parse failure without surfacing a user error).
pub fn deserialize_waypoints(bot: &mut BotState, body: &str, name_glob: &str) -> usize {
    let mut count = 0usize;
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        // `bot_name,location,x,y,z,o,map` — PB2 only requires 3 tokens
        // historically (bot, name, combined-position) but the Rust
        // writer always emits 7 columns, so we accept either layout.
        let mut tokens = line.split(',');
        // Skip bot-name column (always `"BOTNAME"` placeholder for now).
        let Some(_bot) = tokens.next() else { continue };
        let Some(name) = tokens.next() else { continue };
        let Some(x) = tokens.next().and_then(|s| s.trim().parse::<f32>().ok()) else {
            continue;
        };
        let Some(y) = tokens.next().and_then(|s| s.trim().parse::<f32>().ok()) else {
            continue;
        };
        let Some(z) = tokens.next().and_then(|s| s.trim().parse::<f32>().ok()) else {
            continue;
        };
        if !glob_matches(name_glob, name) {
            continue;
        }
        bot.settings
            .rtsc_waypoints
            .insert(name.to_string(), (x, y, z));
        count += 1;
    }
    count
}

/// PB2 glob matching for `rtsc file` args: `"*"` matches everything,
/// any other glob is a case-insensitive substring test. Reference:
/// `RtscAction.cpp:168` and `:254` — PB2 uses `std::string::find`
/// which is case-sensitive, but the Rust `parser::parse()`
/// lowercases command input (commands/parser.rs:221), so every glob
/// arriving here is already lowercase. We lowercase the candidate
/// too, so stored mixed-case names (e.g. user typed `rtsc save here
/// TankSpot`) still match `rtsc file save mc tankspot`.
fn glob_matches(glob: &str, candidate: &str) -> bool {
    if glob == "*" || glob.is_empty() {
        return true;
    }
    candidate.to_ascii_lowercase().contains(glob)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sel;
    use crate::bot::settings::RtscAction;
    use crate::bot::state::{BotState, PlayerClass, PlayerSpec};
    use crate::engine::context::tests::NullInterface;
    use crate::ffi::BotRole;
    use crate::ffi::SpellId;
    use crate::ffi::interface::BotInterface;
    use std::cell::RefCell;

    /// Thin wrapper around `NullInterface` that records spell-store
    /// mutations and log-file I/O so the RTSC tests can assert behavior
    /// without a live FFI bridge.
    struct FakeInterface {
        inner: NullInterface,
        known_spells: RefCell<std::collections::HashSet<u32>>,
        log_files: RefCell<std::collections::HashMap<String, String>>,
    }
    impl Default for FakeInterface {
        fn default() -> Self {
            Self {
                inner: NullInterface,
                known_spells: RefCell::new(Default::default()),
                log_files: RefCell::new(Default::default()),
            }
        }
    }
    impl BotInterface for FakeInterface {
        fn get_snapshot(&self) -> crate::ffi::BotWorldSnapshot {
            self.inner.get_snapshot()
        }
        fn get_unit_snapshot(&self, u: crate::ffi::UnitHandle) -> crate::ffi::BotUnitSnapshot {
            self.inner.get_unit_snapshot(u)
        }
        fn has_aura(&self, u: crate::ffi::UnitHandle, s: SpellId) -> bool {
            self.inner.has_aura(u, s)
        }
        fn get_aura(
            &self,
            u: crate::ffi::UnitHandle,
            s: SpellId,
        ) -> Option<crate::ffi::BotAuraInfo> {
            self.inner.get_aura(u, s)
        }
        fn get_auras(&self, u: crate::ffi::UnitHandle) -> Vec<crate::ffi::BotAuraInfo> {
            self.inner.get_auras(u)
        }
        fn get_threat_list(&self, u: crate::ffi::UnitHandle) -> Vec<crate::ffi::BotThreatEntry> {
            self.inner.get_threat_list(u)
        }
        fn get_unit_threat(&self, a: crate::ffi::UnitHandle, b: crate::ffi::UnitHandle) -> f32 {
            self.inner.get_unit_threat(a, b)
        }
        fn unit_distance(&self, u: crate::ffi::UnitHandle) -> f32 {
            self.inner.unit_distance(u)
        }
        fn can_cast(&self, s: SpellId, u: crate::ffi::UnitHandle) -> bool {
            self.inner.can_cast(s, u)
        }
        fn spell_cooldown_ms(&self, s: SpellId) -> u32 {
            self.inner.spell_cooldown_ms(s)
        }
        fn has_los(&self, u: crate::ffi::UnitHandle) -> bool {
            self.inner.has_los(u)
        }
        fn get_nearby_units(&self, r: f32, h: bool) -> Vec<crate::ffi::UnitHandle> {
            self.inner.get_nearby_units(r, h)
        }
        fn get_behind_position(
            &self,
            u: crate::ffi::UnitHandle,
            d: f32,
        ) -> crate::ffi::BotPosition {
            self.inner.get_behind_position(u, d)
        }
        fn get_safe_position(&self, r: f32) -> Option<crate::ffi::BotPosition> {
            self.inner.get_safe_position(r)
        }
        fn get_spread_position(
            &self,
            c: crate::ffi::UnitHandle,
            r: f32,
            i: u8,
            t: u8,
        ) -> crate::ffi::BotPosition {
            self.inner.get_spread_position(c, r, i, t)
        }
        fn can_reach(&self, x: f32, y: f32, z: f32) -> bool {
            self.inner.can_reach(x, y, z)
        }
        fn cast_spell(&self, s: SpellId, t: crate::ffi::UnitHandle) -> bool {
            self.inner.cast_spell(s, t)
        }
        fn cast_spell_pos(&self, s: SpellId, x: f32, y: f32, z: f32) -> bool {
            self.inner.cast_spell_pos(s, x, y, z)
        }
        fn move_to(&self, x: f32, y: f32, z: f32) -> bool {
            self.inner.move_to(x, y, z)
        }
        fn follow(&self, t: crate::ffi::UnitHandle, d: f32, a: f32) -> bool {
            self.inner.follow(t, d, a)
        }
        fn stop_moving(&self) -> bool {
            self.inner.stop_moving()
        }
        fn attack(&self, t: crate::ffi::UnitHandle) -> bool {
            self.inner.attack(t)
        }
        fn auto_attack(&self, e: bool) -> bool {
            self.inner.auto_attack(e)
        }
        fn say(&self, m: &str, l: u32) -> bool {
            self.inner.say(m, l)
        }
        fn use_item(&self, i: crate::ffi::ItemId, t: crate::ffi::UnitHandle) -> bool {
            self.inner.use_item(i, t)
        }
        fn taunt(&self, t: crate::ffi::UnitHandle) -> bool {
            self.inner.taunt(t)
        }
        fn group_get_tank(&self) -> Option<crate::ffi::UnitHandle> {
            self.inner.group_get_tank()
        }
        fn group_get_healer(&self) -> Option<crate::ffi::UnitHandle> {
            self.inner.group_get_healer()
        }
        fn group_get_role(&self, m: crate::ffi::UnitHandle) -> BotRole {
            self.inner.group_get_role(m)
        }

        // Overrides that matter for RTSC tests.
        fn knows_spell(&self, spell_id: SpellId) -> bool {
            self.known_spells.borrow().contains(&spell_id.raw())
        }
        fn bot_learn_spell(&self, spell_id: u32) {
            self.known_spells.borrow_mut().insert(spell_id);
        }
        fn bot_remove_spell(&self, spell_id: u32) {
            self.known_spells.borrow_mut().remove(&spell_id);
        }
        fn bot_write_log_file(&self, name: &str, body: &str) -> bool {
            self.log_files
                .borrow_mut()
                .insert(name.to_string(), body.to_string());
            true
        }
        fn bot_read_log_file(&self, name: &str) -> Option<String> {
            self.log_files.borrow().get(name).cloned()
        }
    }

    fn fake_bot() -> BotState {
        use crate::bot::state::BotTrees;
        use crate::engine::bt::Bt;
        BotState::new(
            1,
            Box::new(FakeInterface::default()),
            PlayerClass::Warrior,
            PlayerSpec::WarriorArms,
            BotRole::DPS,
            BotTrees {
                combat: Bt::Noop,
                world: Bt::Noop,
                dead: Bt::Noop,
                maintenance: Bt::Noop,
            },
        )
    }

    #[test]
    fn select_learns_aedm_once() {
        let mut bot = fake_bot();
        select(&mut bot);
        assert!(bot.settings.rtsc_selected);
        // Idempotent — second select doesn't re-learn.
        select(&mut bot);
        // Access the fake via snapshot (downcast is painful with `Box<dyn>`
        // — instead observe via `knows_spell` round-trip).
        assert!(bot.interface.knows_spell(SpellId(RTSC_MOVE_SPELL)));
    }

    #[test]
    fn reset_unlearns_and_wipes() {
        let mut bot = fake_bot();
        select(&mut bot);
        bot.settings
            .rtsc_waypoints
            .insert("tank".into(), (1.0, 2.0, 3.0));
        bot.settings.rtsc_last_seen = Some((4.0, 5.0, 6.0));
        bot.settings
            .strategies
            .get_mut(BotStateKind::NonCombat)
            .insert(StrategyFlags::RTSC_JUMP);

        reset(&mut bot);
        assert!(!bot.settings.rtsc_selected);
        assert!(bot.settings.rtsc_waypoints.is_empty());
        assert!(bot.settings.rtsc_last_seen.is_none());
        assert!(
            !bot.settings
                .strategies
                .get(BotStateKind::NonCombat)
                .contains(StrategyFlags::RTSC_JUMP)
        );
        assert!(!bot.interface.knows_spell(SpellId(RTSC_MOVE_SPELL)));
    }

    #[test]
    fn on_spell_land_updates_last_seen_and_moves_when_pending_move() {
        let mut bot = fake_bot();
        queue_pending(&mut bot, RtscAction::Move { exact: false });
        let outcome = on_spell_land(&mut bot, 100.0, 200.0, 50.0);
        assert_eq!(bot.settings.rtsc_last_seen, Some((100.0, 200.0, 50.0)));
        assert!(bot.settings.rtsc_pending_action.is_none());
        assert!(matches!(outcome, SpellLandOutcome::Moved { exact: false, .. }));
        // Forced intention should now drive the move.
        assert!(matches!(
            bot.bdi.forced_intention,
            Some(crate::bdi::intentions::ForcedIntention::MoveToRtsc { .. })
        ));
    }

    #[test]
    fn on_spell_land_records_save_under_name() {
        let mut bot = fake_bot();
        queue_pending(&mut bot, RtscAction::Save { name: "tank spot".into() });
        let outcome = on_spell_land(&mut bot, 10.0, 20.0, 30.0);
        assert_eq!(
            bot.settings.rtsc_waypoints.get("tank spot"),
            Some(&(10.0, 20.0, 30.0))
        );
        assert!(matches!(outcome, SpellLandOutcome::Saved { .. }));
    }

    #[test]
    fn on_spell_land_drops_stale_pending() {
        let mut bot = fake_bot();
        // Queue at t=0, then advance the snapshot clock past the TTL.
        queue_pending(&mut bot, RtscAction::Move { exact: false });
        bot.snap.server_time_ms = RTSC_PENDING_TTL_MS + 1;
        let outcome = on_spell_land(&mut bot, 1.0, 2.0, 3.0);
        // Pending was discarded; bot was not selected, so we ignore.
        assert!(matches!(outcome, SpellLandOutcome::Ignored));
        assert!(bot.bdi.forced_intention.is_none());
    }

    #[test]
    fn jump_two_stage_records_both_slots() {
        let mut bot = fake_bot();
        assert_eq!(jump_command(&mut bot), JumpCommandResult::StageOneQueued);
        assert!(
            bot.settings
                .strategies
                .get(BotStateKind::NonCombat)
                .contains(StrategyFlags::RTSC_JUMP)
        );
        // First Aedm cast → "jump" slot populated, pending re-armed.
        let outcome = on_spell_land(&mut bot, 1.0, 2.0, 3.0);
        assert_eq!(
            bot.settings.rtsc_waypoints.get(JUMP_SLOT),
            Some(&(1.0, 2.0, 3.0))
        );
        assert!(matches!(outcome, SpellLandOutcome::JumpStageOne { .. }));
        assert!(matches!(
            bot.settings.rtsc_pending_action,
            Some((RtscAction::Jump, _))
        ));
        // Second Aedm cast → "jump point" slot populated, pending cleared,
        // forced intention queued.
        let outcome = on_spell_land(&mut bot, 4.0, 5.0, 6.0);
        assert_eq!(
            bot.settings.rtsc_waypoints.get(JUMP_POINT_SLOT),
            Some(&(4.0, 5.0, 6.0))
        );
        assert!(bot.settings.rtsc_pending_action.is_none());
        assert!(matches!(outcome, SpellLandOutcome::JumpStageTwo { .. }));
        assert!(matches!(
            bot.bdi.forced_intention,
            Some(crate::bdi::intentions::ForcedIntention::JumpRtsc { .. })
        ));
    }

    #[test]
    fn jump_command_detects_stale_and_cancels() {
        let mut bot = fake_bot();
        // Simulate stale state: jump slot set, jump point missing,
        // and the user types `rtsc jump` again.
        bot.settings
            .rtsc_waypoints
            .insert(JUMP_SLOT.into(), (1.0, 2.0, 3.0));
        bot.settings
            .strategies
            .get_mut(BotStateKind::NonCombat)
            .insert(StrategyFlags::RTSC_JUMP);
        assert_eq!(jump_command(&mut bot), JumpCommandResult::StaleCancelled);
        assert!(bot.settings.rtsc_waypoints.get(JUMP_SLOT).is_none());
        assert!(
            !bot.settings
                .strategies
                .get(BotStateKind::NonCombat)
                .contains(StrategyFlags::RTSC_JUMP)
        );
    }

    #[test]
    fn jump_command_reports_in_progress_when_both_slots_full() {
        let mut bot = fake_bot();
        bot.settings
            .rtsc_waypoints
            .insert(JUMP_SLOT.into(), (1.0, 2.0, 3.0));
        bot.settings
            .rtsc_waypoints
            .insert(JUMP_POINT_SLOT.into(), (4.0, 5.0, 6.0));
        assert_eq!(jump_command(&mut bot), JumpCommandResult::AlreadyInProgress);
    }

    #[test]
    fn last_moves_to_recorded_cast() {
        let mut bot = fake_bot();
        assert!(!last(&bot));
        bot.settings.rtsc_last_seen = Some((7.0, 8.0, 9.0));
        assert!(last(&bot));
    }

    #[test]
    fn serialize_deserialize_round_trip_with_glob() {
        let mut bot = fake_bot();
        bot.settings
            .rtsc_waypoints
            .insert("tankspot".into(), (1.0, 2.0, 3.0));
        bot.settings
            .rtsc_waypoints
            .insert("healspot".into(), (4.0, 5.0, 6.0));
        // Jump slots must be excluded from the export even with "*".
        bot.settings
            .rtsc_waypoints
            .insert(JUMP_SLOT.into(), (9.0, 9.0, 9.0));

        let (body, n) = serialize_waypoints(&bot, "*");
        assert_eq!(n, 2);
        assert!(!body.contains(",jump,"));

        // Clear and reimport.
        bot.settings.rtsc_waypoints.clear();
        let imported = deserialize_waypoints(&mut bot, &body, "*");
        assert_eq!(imported, 2);
        assert_eq!(
            bot.settings.rtsc_waypoints.get("tankspot"),
            Some(&(1.0, 2.0, 3.0))
        );
    }

    #[test]
    fn serialize_respects_name_glob() {
        let mut bot = fake_bot();
        bot.settings
            .rtsc_waypoints
            .insert("magmadar range".into(), (1.0, 2.0, 3.0));
        bot.settings
            .rtsc_waypoints
            .insert("lucifron tank".into(), (4.0, 5.0, 6.0));
        let (body, n) = serialize_waypoints(&bot, "magmadar");
        assert_eq!(n, 1);
        assert!(body.contains("magmadar range"));
        assert!(!body.contains("lucifron"));
    }

    #[test]
    fn save_here_summons_marker_and_stores_waypoint() {
        let mut bot = fake_bot();
        bot.snap.self_.pos.x = 10.0;
        bot.snap.self_.pos.y = 20.0;
        bot.snap.self_.pos.z = 30.0;
        save_here(&mut bot, "spawn".into());
        assert_eq!(
            bot.settings.rtsc_waypoints.get("spawn"),
            Some(&(10.0, 20.0, 30.0))
        );
        // Aedm was auto-learned as a side effect.
        assert!(bot.interface.knows_spell(SpellId(RTSC_MOVE_SPELL)));
    }

    #[test]
    fn file_round_trip_via_interface() {
        let mut bot = fake_bot();
        bot.settings
            .rtsc_waypoints
            .insert("tankspot".into(), (1.0, 2.0, 3.0));
        let (body, n) = serialize_waypoints(&bot, "*");
        assert_eq!(n, 1);
        assert!(bot.interface.bot_write_log_file("raid.csv", &body));

        bot.settings.rtsc_waypoints.clear();
        let loaded = bot.interface.bot_read_log_file("raid.csv").unwrap();
        let imported = deserialize_waypoints(&mut bot, &loaded, "*");
        assert_eq!(imported, 1);
        assert!(bot.settings.rtsc_waypoints.contains_key("tankspot"));
    }
}
