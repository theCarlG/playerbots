//! Background random-manager worker thread.
//!
//! Owns a [`RandomMgrState`] and an `Arc<dyn RandomMgrWorld>`, driven
//! by messages from the main thread over an `mpsc::channel`. The main
//! thread sends `WorkerRequest::Tick` once per world-tick with the
//! current wall-clock; the worker pulls a fresh [`TickConfig`] from
//! the global [`crate::config`], runs [`tick`], and sends back a
//! [`TickStats`] the main thread logs.
//!
//! Mirrors `login/worker.rs` verbatim — same channel protocol, same
//! shutdown path, same `Drop` semantics. The only interesting bits are:
//!
//! * State lives on the worker thread; FFI entry points that need to
//!   read/write it (activity percentage, reset, cache load) speak the
//!   [`WorkerRequest`] protocol via the handle.
//! * `TickConfig::from_bot_config` is called inside the worker loop
//!   so `/bot reload` changes are picked up on the next tick.
//! * The worker thread reports its last stats back through a
//!   response channel — the main thread drains them non-blocking.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use cmangos::RandomMgrWorld;

use crate::config;

use super::state::RandomMgrState;
use super::update_loop::{TickConfig, TickStats, sync_event_timers, tick};

/// One message sent from the main thread to the worker. All variants
/// have exactly-once delivery semantics.
#[derive(Debug)]
pub enum WorkerRequest {
    /// Drive one [`tick`] pass. `now_epoch_s` is supplied by the caller
    /// so the worker doesn't depend on wall-clock monotonicity across
    /// threads.
    Tick { now_epoch_s: u32 },
    /// Set the `activityMod` scaling factor directly. Called from
    /// `/rndbot activity <pct>` chat command handlers.
    SetActivityPercentage(f32),
    /// Wipe every piece of in-memory state (equivalent to the console
    /// `reset` command's first half — the DB-side delete fires from the
    /// main thread because the worker doesn't own the `RandomMgrWorld`
    /// trait in a DB-writable way for one-off resets).
    Reset,
    /// Run [`sync_event_timers`] once. Called on first startup after
    /// the event cache has been hydrated.
    SyncEventTimers { now_epoch_s: u32 },
    /// Re-tune the PID controller gains. Called from the
    /// `rndbot pid <p> <i> <d>` console command.
    AdjustPid { p: f64, i: f64, d: f64 },
    /// Stop the worker. The thread exits as soon as it pops this
    /// message.
    Shutdown,
}

/// One message sent from the worker back to the main thread.
#[derive(Clone, Debug)]
pub enum WorkerResponse {
    /// Result of a [`WorkerRequest::Tick`]. The main thread logs these
    /// and optionally forwards them to a stats sink.
    Tick(TickStats),
    /// Snapshot of `state.activity_percentage()` after processing a
    /// [`WorkerRequest::SetActivityPercentage`].
    ActivitySet(f32),
    /// `Reset` acknowledgement — signals the state has been wiped.
    ResetDone,
    /// `SyncEventTimers` acknowledgement.
    SyncEventTimersDone,
    /// `AdjustPid` acknowledgement.
    PidAdjusted,
}

/// Handle the main thread uses to talk to the spawned worker. Wraps
/// the request channel, the response channel, the shared world handle,
/// and the worker's `JoinHandle` — `Drop` sends a `Shutdown` and joins
/// the thread.
pub struct RandomMgrWorkerHandle {
    tx: Sender<WorkerRequest>,
    rx: Receiver<WorkerResponse>,
    world: Arc<dyn RandomMgrWorld>,
    thread: Option<JoinHandle<()>>,
}

impl RandomMgrWorkerHandle {
    /// Spawn the worker thread. The worker seeds its own
    /// [`RandomMgrState`] via `RandomMgrState::new()` and owns it for
    /// the lifetime of the thread.
    #[must_use]
    pub fn spawn(world: Arc<dyn RandomMgrWorld>) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<WorkerRequest>();
        let (resp_tx, resp_rx) = mpsc::channel::<WorkerResponse>();

        let worker_world = Arc::clone(&world);
        let thread = thread::Builder::new()
            .name("playerbot-random-mgr".into())
            .spawn(move || {
                worker_loop(worker_world, &req_rx, &resp_tx);
            })
            .expect("failed to spawn playerbot random-mgr worker");

        Self {
            tx: req_tx,
            rx: resp_rx,
            world,
            thread: Some(thread),
        }
    }

    /// Access the shared world handle — used by the main thread when
    /// it needs to call trait methods directly (e.g. for chat commands
    /// that bypass the worker).
    #[must_use]
    pub fn world(&self) -> &Arc<dyn RandomMgrWorld> {
        &self.world
    }

    /// Send a `Tick` request. Returns `false` if the worker has exited.
    pub fn send_tick(&self, now_epoch_s: u32) -> bool {
        self.tx.send(WorkerRequest::Tick { now_epoch_s }).is_ok()
    }

    /// Send a `SetActivityPercentage` request. Returns `false` if the
    /// worker has exited.
    pub fn set_activity_percentage(&self, percentage: f32) -> bool {
        self.tx
            .send(WorkerRequest::SetActivityPercentage(percentage))
            .is_ok()
    }

    /// Send a `Reset` request. Returns `false` if the worker has exited.
    pub fn reset(&self) -> bool {
        self.tx.send(WorkerRequest::Reset).is_ok()
    }

    /// Send a `SyncEventTimers` request. Returns `false` if the worker
    /// has exited.
    pub fn sync_event_timers(&self, now_epoch_s: u32) -> bool {
        self.tx
            .send(WorkerRequest::SyncEventTimers { now_epoch_s })
            .is_ok()
    }

    /// Send an `AdjustPid` request. Returns `false` if the worker has
    /// exited.
    pub fn adjust_pid(&self, p: f64, i: f64, d: f64) -> bool {
        self.tx.send(WorkerRequest::AdjustPid { p, i, d }).is_ok()
    }

    /// Block until the next response arrives. Used by tests for
    /// deterministic sequencing; production code uses
    /// [`drain_responses`](Self::drain_responses).
    #[cfg(test)]
    pub fn recv_response(&self) -> Option<WorkerResponse> {
        self.rx.recv().ok()
    }

    /// Drain any ready worker responses without blocking.
    #[must_use]
    pub fn drain_responses(&self) -> Vec<WorkerResponse> {
        let mut out = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            out.push(msg);
        }
        out
    }
}

impl Drop for RandomMgrWorkerHandle {
    fn drop(&mut self) {
        // Best-effort: send shutdown. If the receiver is already gone
        // the worker has exited on its own.
        let _ = self.tx.send(WorkerRequest::Shutdown);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Core loop run by the worker thread. Exits on `Shutdown` or when
/// the request channel closes. The first tick pulls a fresh config
/// snapshot from [`crate::config`] each time so `/bot reload` is
/// picked up without restarting the worker.
fn worker_loop(
    world: Arc<dyn RandomMgrWorld>,
    req_rx: &Receiver<WorkerRequest>,
    resp_tx: &Sender<WorkerResponse>,
) {
    let mut state = RandomMgrState::new();

    while let Ok(msg) = req_rx.recv() {
        match msg {
            WorkerRequest::Tick { now_epoch_s } => {
                let cfg = config::get();
                let tick_cfg = TickConfig::from_bot_config(&cfg);
                let stats = tick(&mut state, world.as_ref(), &tick_cfg, now_epoch_s);
                if resp_tx.send(WorkerResponse::Tick(stats)).is_err() {
                    // Main thread dropped the receiver — shut down.
                    break;
                }
            }
            WorkerRequest::SetActivityPercentage(pct) => {
                state.set_activity_percentage(pct);
                let pct_now = state.activity_percentage();
                if resp_tx.send(WorkerResponse::ActivitySet(pct_now)).is_err() {
                    break;
                }
            }
            WorkerRequest::Reset => {
                state.reset_in_memory();
                if resp_tx.send(WorkerResponse::ResetDone).is_err() {
                    break;
                }
            }
            WorkerRequest::AdjustPid { p, i, d } => {
                state.pid.adjust(p, i, d);
                if resp_tx.send(WorkerResponse::PidAdjusted).is_err() {
                    break;
                }
            }
            WorkerRequest::SyncEventTimers { now_epoch_s } => {
                sync_event_timers(&mut state, world.as_ref(), now_epoch_s);
                if resp_tx.send(WorkerResponse::SyncEventTimersDone).is_err() {
                    break;
                }
            }
            WorkerRequest::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigState, RawConfig, TEST_LOCK};
    use crate::config::typed::BotConfig;
    use cmangos::{MockRandomMgrEvent, MockRandomMgrWorld, WorldDiffSample};

    fn install_cfg() {
        let mut cfg = BotConfig::default();
        cfg.random_bot_autologin = true;
        cfg.min_random_bots = 5;
        cfg.max_random_bots = 10;
        cfg.random_bot_count_change_min_interval = 60;
        cfg.random_bot_count_change_max_interval = 120;
        cfg.sync_level_no_player = 25;
        cfg.sync_level_with_players = true;
        cfg.diff_empty = 100;
        cfg.diff_with_player = 150;
        let raw = RawConfig::default();
        config::install(ConfigState { raw, typed: cfg });
    }

    fn mk_world() -> Arc<MockRandomMgrWorld> {
        let world = Arc::new(MockRandomMgrWorld::new());
        world.set_world_diff_sample(WorldDiffSample {
            current_diff_ms: 50.0,
            average_diff_ms: 50.0,
            max_diff_ms: 50.0,
        });
        world
    }

    #[test]
    fn worker_processes_tick_and_emits_stats() {
        let _g = TEST_LOCK.lock().unwrap();
        install_cfg();

        let mock = mk_world();
        mock.set_owned_guids(vec![1, 2, 3]);

        // Prime `add` events for each bot so tick's randomize path
        // fires instead of logout.
        let state_world: Arc<dyn RandomMgrWorld> = mock.clone();
        {
            use super::super::events::EventCache;
            let mut ec = EventCache::new();
            ec.set_value(1, "add", 1, 10_000, "", 0, state_world.as_ref());
            ec.set_value(2, "add", 1, 10_000, "", 0, state_world.as_ref());
            ec.set_value(3, "add", 1, 10_000, "", 0, state_world.as_ref());
            // We also need the worker's own state cache to have these
            // events. The worker seeds its own cache from scratch and
            // would otherwise re-query the world for each GetValue. The
            // MockRandomMgrWorld's `query_events_for_bot` returns our
            // in-memory upserts, so the worker will pick them up on
            // first lazy hydrate.
        }

        let handle = RandomMgrWorkerHandle::spawn(state_world);
        assert!(handle.send_tick(1000));
        let resp = handle.recv_response().expect("tick response");
        let stats = match resp {
            WorkerResponse::Tick(s) => s,
            other => panic!("unexpected response: {:?}", other),
        };
        assert_eq!(stats.bots_processed, 3);
    }

    #[test]
    fn worker_exits_on_shutdown_message() {
        let _g = TEST_LOCK.lock().unwrap();
        install_cfg();

        let mock = mk_world();
        let handle = RandomMgrWorkerHandle::spawn(mock);
        // Drop without sending a tick should still cleanly join.
        drop(handle);
    }

    #[test]
    fn worker_set_activity_percentage_round_trips() {
        let _g = TEST_LOCK.lock().unwrap();
        install_cfg();

        let mock = mk_world();
        let handle = RandomMgrWorkerHandle::spawn(mock);
        assert!(handle.set_activity_percentage(75.0));
        match handle.recv_response() {
            Some(WorkerResponse::ActivitySet(v)) => assert!((v - 75.0).abs() < 1e-4),
            other => panic!("expected ActivitySet, got {:?}", other),
        }
    }

    #[test]
    fn worker_reset_wipes_and_acks() {
        let _g = TEST_LOCK.lock().unwrap();
        install_cfg();

        let mock = mk_world();
        let handle = RandomMgrWorkerHandle::spawn(mock);
        assert!(handle.set_activity_percentage(80.0));
        let _ = handle.recv_response();
        assert!(handle.reset());
        match handle.recv_response() {
            Some(WorkerResponse::ResetDone) => {}
            other => panic!("expected ResetDone, got {:?}", other),
        }
    }

    #[test]
    fn worker_adjust_pid_acks() {
        let _g = TEST_LOCK.lock().unwrap();
        install_cfg();

        let mock = mk_world();
        let handle = RandomMgrWorkerHandle::spawn(mock);
        assert!(handle.adjust_pid(0.05, 0.001, 0.05));
        match handle.recv_response() {
            Some(WorkerResponse::PidAdjusted) => {}
            other => panic!("expected PidAdjusted, got {:?}", other),
        }
    }

    #[test]
    fn worker_sync_event_timers_hits_world_bump() {
        let _g = TEST_LOCK.lock().unwrap();
        install_cfg();

        let mock = mk_world();
        let world_for_upsert: Arc<dyn RandomMgrWorld> = mock.clone();
        // Pre-seed `current_time` event via the trait so the worker's
        // lazy hydration picks it up.
        use super::super::events::EventCache;
        let mut seed = EventCache::new();
        seed.set_value(0, "current_time", 1000, 0, "", 0, world_for_upsert.as_ref());

        let handle = RandomMgrWorkerHandle::spawn(world_for_upsert);
        assert!(handle.sync_event_timers(1500));
        match handle.recv_response() {
            Some(WorkerResponse::SyncEventTimersDone) => {}
            other => panic!("expected SyncEventTimersDone, got {:?}", other),
        }
        drop(handle);
        let events = mock.events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockRandomMgrEvent::BumpEventTimes(500))),
            "expected BumpEventTimes(500)"
        );
    }
}
