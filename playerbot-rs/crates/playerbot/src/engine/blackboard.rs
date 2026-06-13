/// Per-bot typed key-value store.
///
/// All state that the BT writes and reads across ticks lives here.
/// O(1) access via enum discriminant index — no string lookups, no `HashMap`.
use cmangos::UnitHandle;

/// All keys a BT node can read or write.
/// Each variant is an index into a fixed-size array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Key {
    LastAttackTarget = 0,
    CombatStartMs,
    CurrentEncounterPhase, // u32 encoding of the phase enum for the active encounter
    AssignedRoleOverride,  // BotRole override from group coordinator
    InProgressActionId,    // id of a Running action (movement, casting)
    InProgressActionData,  // context data for the in-progress action
    FollowTargetHandle,
    LastSafePositionX,
    LastSafePositionY,
    LastSafePositionZ,
    LastFleeMs,        // server_time_ms of last flee movement
    ThreatResetNeeded, // bool: bot lost aggro, should stop attacking
    PullTimeMs,        // server_time_ms when the current boss was pulled
    AddCount,          // number of active adds (for encounter scripts)
    EncounterSafeZone, // u32: Heigan safe zone (1-4), 0 if not applicable

    // ── World behavior keys ─────────────────────────────────────────
    TravelDestX, // f32: travel destination
    TravelDestY,
    TravelDestZ,
    TravelDestMap, // u32: destination map (for cross-continent routing)
    GrindTargetHandle, // handle: current grind target
    LastVendorVisitMs, // u64: when we last vendored
    LastRepairMs,      // u64: when we last repaired
    LastConsumeMs,     // u64: when we last used a food/drink item (eat/drink throttle)
    TravelFailStreak,  // u32: consecutive TravelToBlackboard move_to failures
    PreferNearUntilMs, // u64: until this time, skip far quest-routing (anti-freeze)
    EngageTargetHandle, // u64: quest-objective mob the bot wants to fight; the
    // tick loop promotes it to focus_target so the combat FSM engages it
    // (handles NEUTRAL quest mobs that never trigger combat on their own)
    FocusProgressMs,  // u64: last time the bot made combat progress on its focus
    // (in combat / moving / casting). Used to abandon an unreachable focus.
    BadTargetHandle,  // u64: a focus target abandoned as unreachable...
    BadTargetUntilMs, // u64: ...skip re-picking it until this time
    // Anti-deadlock watchdog anchor: the last time/place/HP/mana the bot made
    // real PROGRESS (moved, fought, cast, or recovered). If none of that happens
    // for too long the bot is deadlocked — the watchdog force-breaks its stuck
    // state so it never just freezes with no purpose.
    WedgeAnchorX,     // f32: position at last progress
    WedgeAnchorY,     // f32
    WedgeAnchorMs,    // u64: time of last progress
    IdleAnchorHp,     // f32: hp% at last progress (rising hp = recovering = progress)
    IdleAnchorMana,   // f32: mana% at last progress (rising mana = drinking = progress)
    TownErrandUntilMs, // u64: while > now the bot is on a "town visit" — batch ALL
    // town errands (turn in quests, sell, repair) before returning to questing,
    // so a trip to town feels intentional instead of one-need-then-leave
    RecoverActiveMs,  // u64: last tick the eat/drink behavior was actively recovering
    // (purposeful drink/eat) — distinguishes it from passive idle mana regen so
    // the anti-deadlock watchdog doesn't treat idle regen as "making progress"
    QuestTakerFutileUntilMs, // u64: while > now, DON'T travel to quest turn-ins.
    // Set when the bot is already within turn-in range of a turn-in dest but
    // quest_subtree's TurnInQuest couldn't hand anything in (the NPC there
    // doesn't accept the bot's complete quest(s)). Stops the bot bouncing
    // between unactionable turn-in spots in town instead of working objectives.

    // ── Travel progress tracking + unreachable-dest blacklist ───────
    // Detect a travel dest the bot can't actually reach (e.g. a quest objective
    // just past the navmesh edge): move_to keeps returning true as it re-paths
    // to the mesh edge, so the bot never arrives and never bails — it circles
    // forever ~30y short. We track the closest distance reached toward the
    // CURRENT dest; if it doesn't improve for ~12s the dest is unreachable, so
    // we blacklist its location and pick a different one.
    TravelTrackX,      // f32: dest we're tracking progress toward
    TravelTrackY,      // f32
    TravelMinDistSq,   // f32: closest dist^2 reached toward the tracked dest
    TravelProgressMs,  // u64: last time that closest distance improved
    // Small ring of recently-failed (unreachable/unactionable) dest locations.
    // ChooseTravelTarget skips candidates within ~12y of a non-expired entry so
    // the bot tries a DIFFERENT objective/service instead of re-picking the same
    // bad spot. 3 slots so a couple of bad dests can be held at once.
    BlDestX0, BlDestY0, BlDestMs0,
    BlDestX1, BlDestY1, BlDestMs1,
    BlDestX2, BlDestY2, BlDestMs2,
    BlDestIdx, // u32: next ring slot to overwrite

    // Per-quest turn-in cooldown ring: a quest whose hand-in the core rejects at
    // the NPC (CanRewardQuest false — e.g. a deliver-item the bot no longer has,
    // or a script-completed quest) is parked here so the bot stops spamming that
    // turn-in (and bouncing between the inn's NPCs) and works other quests.
    TurnInCdQ0, TurnInCdMs0,
    TurnInCdQ1, TurnInCdMs1,
    TurnInCdQ2, TurnInCdMs2,
    TurnInCdIdx, // u32: next ring slot to overwrite
    // Which complete quest the bot is currently trying to hand in, and since
    // when — so a turn-in that's stuck APPROACHING an unreachable NPC (never
    // arrives to actually fail) still gets parked after a few seconds.
    TurnInTryQ,  // u32
    TurnInTryMs, // u64

    // Interaction "dwell": a real player takes a moment to talk to an NPC, open
    // a corpse, or use a quest object — bots did it instantly. We hold at the
    // target for a short beat before completing the action. Keyed on the target
    // GUID so switching targets restarts the timer.
    ActionDwellTarget, // u64
    ActionDwellSince,  // u64

    // ── RPG mode keys ───────────────────────────────────────────────
    RpgWanderDestX, // f32: current wander destination, held until arrival
    RpgWanderDestY,
    RpgWanderDestZ,
    // f32: where the bot STARTED the current wander leg, so the next leg avoids
    // backtracking straight to it (stops the "paces between two points" look).
    RpgWanderFromX,
    RpgWanderFromY,
    RpgVisitDestX, // f32: town NPC the bot is strolling over to visit, held until reached
    RpgVisitDestY,
    RpgVisitDestZ,
    FishSpotX, // f32: fishing spot the bot is walking to, held until reached
    FishSpotY,
    FishSpotZ,

    // ── Formation (chaos) persistence ───────────────────────────────
    // Per-bot state for the `chaos` follow-formation jitter. Rerolled
    // every 3 seconds by `tick_follow`; other formations leave these
    // untouched. See `bot::formation::ChaosState`.
    ChaosDx,             // f32
    ChaosDy,             // f32
    ChaosLastChangeSecs, // u64

    // ── Death behavior ───────────────────────────────────────────────
    DeathTimestampMs, // u64: server_time_ms when the bot died (0 = alive)

    // ── Follow throttle ──────────────────────────────────────────────
    LastFollowMs, // u64: last time position-based follow issued move_to

    // ── FSM state ─────────────────────────────────────────────────────
    ActiveFsmState, // u32: ActiveFsm discriminant (0=World, 1=Combat, 2=Dead)
    WorldSubState,  // u32: WorldSub discriminant (Follow/Grind/Quest/Stay/...)

    // ── Pull-back state ─────────────────────────────────────────────
    IsPulling,      // u32(1) when the bot has just pulled and is returning to group
    PullBackX,      // f32: pre-pull position to return to during pull-back
    PullBackY,      // f32
    PullBackZ,      // f32
    PullStartTime,  // u32: server_time_ms when IsPulling was set (for timeout)

    // ── BDI / GOAP state ──────────────────────────────────────────
    BdiActiveDesire,             // u32: DesireKind discriminant
    BdiIntentionChangedThisTick, // bool: intention switched this tick
    GoapCurrentAction,           // u32: ActionId of current GOAP plan step
    GoapPlanStep,                // u32: which step we're on (0-based)
    GoapPlanLength,              // u32: total plan length
    AiLodTier,                   // u32: AiLod discriminant (0=Full..3=Dormant)
    GoapStepCompleteSignal,      // bool: BT signaled current step complete
    GoapStepFailedSignal,        // bool: BT signaled current step failed

    // Add new keys above this line. Keep count accurate.
    _Count,
}

const BOARD_SIZE: usize = Key::_Count as usize;

/// Value stored in a blackboard slot.
#[derive(Debug, Clone, Copy, Default)]
pub enum Value {
    #[default]
    None,
    Handle(UnitHandle),
    U32(u32),
    U64(u64),
    F32(f32),
    Bool(bool),
}

/// The blackboard itself — a fixed-size array of Values.
#[derive(Debug)]
pub struct Blackboard {
    slots: [Value; BOARD_SIZE],
    /// Temporary monitor log lines accumulated during a tick.
    /// Flushed after the BT tick when monitoring is active.
    monitor_lines: Vec<String>,
}

impl Default for Blackboard {
    fn default() -> Self {
        Self {
            slots: [Value::None; BOARD_SIZE],
            monitor_lines: Vec::new(),
        }
    }
}

impl Blackboard {
    pub fn get(&self, key: Key) -> Value {
        self.slots[key as usize]
    }

    pub fn set(&mut self, key: Key, value: Value) {
        self.slots[key as usize] = value;
    }

    pub fn clear(&mut self, key: Key) {
        self.slots[key as usize] = Value::None;
    }

    /// Push a monitor log line (flushed after the BT tick).
    pub fn push_monitor_line(&mut self, line: String) {
        self.monitor_lines.push(line);
    }

    /// Drain all accumulated monitor lines.
    pub fn drain_monitor_lines(&mut self) -> std::vec::Drain<'_, String> {
        self.monitor_lines.drain(..)
    }

    pub fn get_u32(&self, key: Key) -> Option<u32> {
        match self.get(key) {
            Value::U32(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_u64(&self, key: Key) -> Option<u64> {
        match self.get(key) {
            Value::U64(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_f32(&self, key: Key) -> Option<f32> {
        match self.get(key) {
            Value::F32(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_bool(&self, key: Key) -> Option<bool> {
        match self.get(key) {
            Value::Bool(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_handle(&self, key: Key) -> Option<UnitHandle> {
        match self.get(key) {
            Value::Handle(v) if v != 0 => Some(v),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_u32() {
        let mut bb = Blackboard::default();
        bb.set(Key::AddCount, Value::U32(3));
        assert_eq!(bb.get_u32(Key::AddCount), Some(3));
    }

    #[test]
    fn roundtrip_handle() {
        let mut bb = Blackboard::default();
        let h: UnitHandle = 0xDEAD_BEEF_0000_0001;
        bb.set(Key::LastAttackTarget, Value::Handle(h));
        assert_eq!(bb.get_handle(Key::LastAttackTarget), Some(h));
    }

    #[test]
    fn zero_handle_returns_none() {
        let mut bb = Blackboard::default();
        bb.set(Key::LastAttackTarget, Value::Handle(0));
        assert_eq!(bb.get_handle(Key::LastAttackTarget), None);
    }

    #[test]
    fn clear_resets_to_none() {
        let mut bb = Blackboard::default();
        bb.set(Key::AddCount, Value::U32(5));
        bb.clear(Key::AddCount);
        assert!(matches!(bb.get(Key::AddCount), Value::None));
    }
}
