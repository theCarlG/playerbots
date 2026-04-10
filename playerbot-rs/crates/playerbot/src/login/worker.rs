//! Background login worker thread.
//!
//! Owns a [`LoginManager`] and an `Arc<dyn LoginWorld>` handle, driven
//! by messages from the main thread over an `mpsc::channel`. The main
//! thread sends `WorkerRequest::Tick` once per world-tick with the
//! current real-player snapshot; the worker executes
//! `LoginManager::tick` against its own `LoginWorld` (taking care to
//! refresh the candidate pool on the first tick) and sends back a
//! `TickOutput` the main thread drains into
//! `perform_login`/`perform_logout` calls.
//!
//! The worker lifecycle is:
//!
//! 1. `LoginWorkerHandle::spawn(world)` — creates channels, clones the
//!    `Arc<dyn LoginWorld>`, spawns the thread.
//! 2. Worker loop blocks on `recv()` until a request arrives.
//! 3. On `Tick`: refresh pool if needed, call `LoginManager::tick`, send
//!    the `TickOutput` back.
//! 4. On `ToggleDebug`: flip `manager.debug`.
//! 5. On `Shutdown` (or channel closed): break out of the loop.
//!
//! All mutable state is local to the worker thread — the main thread
//! only sees immutable snapshots via the channels.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use cmangos::LoginWorld;

use crate::config;
use crate::login::manager::{LoginManager, QueuedCommand, TickOutput};
use crate::login::space::RealPlayerSummary;

/// One message sent from the main thread to the worker.
#[derive(Debug)]
pub enum WorkerRequest {
    /// Drive one `LoginManager::tick` against the enclosed real-player
    /// snapshot.
    Tick(Vec<RealPlayerSummary>),
    /// Flip the debug log flag (matches the legacy C++
    /// `ToggleDebug` chat command).
    ToggleDebug,
    /// Stop the worker. The thread exits as soon as it pops this
    /// message.
    Shutdown,
}

/// Handle the main thread uses to talk to the spawned worker. Wraps
/// the request channel, the response channel, and the worker's
/// `JoinHandle` — `Drop` sends a `Shutdown` and joins the thread.
///
/// The handle also stores a shared `Arc<dyn LoginWorld>` so the main
/// thread can dispatch `perform_login`/`perform_logout` commands via
/// [`dispatch_commands`](LoginWorkerHandle::dispatch_commands) without
/// re-wrapping the vtable.
pub struct LoginWorkerHandle {
    tx: Sender<WorkerRequest>,
    rx: Receiver<TickOutput>,
    world: Arc<dyn LoginWorld>,
    thread: Option<JoinHandle<()>>,
}

impl LoginWorkerHandle {
    /// Spawn the worker thread. The worker owns its own `LoginManager`
    /// (seeded from `LoginWorld::query_candidates` on the first tick).
    /// The returned handle lets the main thread post requests and drain
    /// responses.
    #[must_use]
    pub fn spawn(world: Arc<dyn LoginWorld>) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<WorkerRequest>();
        let (resp_tx, resp_rx) = mpsc::channel::<TickOutput>();

        let worker_world = Arc::clone(&world);
        let thread = thread::Builder::new()
            .name("playerbot-login".into())
            .spawn(move || {
                worker_loop(worker_world, &req_rx, &resp_tx);
            })
            .expect("failed to spawn playerbot login worker");

        Self {
            tx: req_tx,
            rx: resp_rx,
            world,
            thread: Some(thread),
        }
    }

    /// Send a `Tick` request to the worker. Returns `false` if the
    /// worker has exited (the channel is closed).
    pub fn send_tick(&self, real_players: Vec<RealPlayerSummary>) -> bool {
        self.tx.send(WorkerRequest::Tick(real_players)).is_ok()
    }

    /// Send a `ToggleDebug` request to the worker. Returns `false` if
    /// the worker has exited.
    pub fn toggle_debug(&self) -> bool {
        self.tx.send(WorkerRequest::ToggleDebug).is_ok()
    }

    /// Block until the next `TickOutput` arrives. Used by tests that
    /// need deterministic sequencing; production code uses
    /// [`drain_tick_output`](LoginWorkerHandle::drain_tick_output).
    #[cfg(test)]
    pub fn recv_tick(&self) -> Option<TickOutput> {
        self.rx.recv().ok()
    }

    /// Drain any ready `TickOutput` messages without blocking. Each
    /// returned entry's commands should be dispatched on the main
    /// thread via [`dispatch_commands`](LoginWorkerHandle::dispatch_commands).
    pub fn drain_tick_output(&self) -> Vec<TickOutput> {
        let mut out = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            out.push(msg);
        }
        out
    }

    /// Main-thread dispatch of a single [`TickOutput`]. Runs the actual
    /// `perform_login`/`perform_logout` via the stored `LoginWorld`.
    pub fn dispatch_commands(&self, output: &TickOutput) {
        for cmd in &output.commands {
            match *cmd {
                QueuedCommand::Login(guid) => {
                    let _ = self.world.perform_login(guid);
                }
                QueuedCommand::Logout(guid) => {
                    let _ = self.world.perform_logout(guid);
                }
            }
        }
    }
}

impl Drop for LoginWorkerHandle {
    fn drop(&mut self) {
        // Best-effort: send shutdown. If the receiver is already gone
        // the worker has exited on its own.
        let _ = self.tx.send(WorkerRequest::Shutdown);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Core loop run by the worker thread. Exits on `Shutdown` or when the
/// request channel closes. The first `Tick` seeds the candidate pool
/// from a fresh `query_candidates` call; subsequent ticks reuse the
/// in-memory pool (the legacy C++ manager only re-queried at startup).
fn worker_loop(
    world: Arc<dyn LoginWorld>,
    req_rx: &Receiver<WorkerRequest>,
    resp_tx: &Sender<TickOutput>,
) {
    let mut manager = LoginManager::new();

    while let Ok(msg) = req_rx.recv() {
        match msg {
            WorkerRequest::Tick(real_players) => {
                let cfg = config::get();
                if !manager.pool_loaded() {
                    let rows = world.query_candidates(&cfg.random_bot_account_prefix);
                    manager.load_pool(&rows, &cfg);
                }
                let out = manager.tick(&cfg, world.as_ref(), &real_players);
                if resp_tx.send(out).is_err() {
                    // Main thread dropped the receiver — shut down.
                    break;
                }
            }
            WorkerRequest::ToggleDebug => {
                manager.debug = !manager.debug;
            }
            WorkerRequest::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TEST_LOCK;
    use cmangos::{BotCandidateInfo, MockLoginEvent, MockLoginWorld};

    fn install_cfg() {
        let mut cfg = crate::config::typed::BotConfig::default();
        cfg.random_bot_account_prefix = "rndbot".into();
        cfg.random_bots_max_logins_per_interval = 10;
        cfg.free_room_for_non_spare_bots = 0;
        cfg.random_bot_min_level = 1;
        cfg.sync_level_max_above = 5;
        cfg.default_login_criteria = vec!["maxbots".into()];
        cfg.login_criteria = vec![vec!["online".into()]];
        let raw = crate::config::RawConfig::default();
        let state = crate::config::ConfigState { raw, typed: cfg };
        crate::config::install(state);
    }

    fn mk_candidate(guid: u32, level: u32) -> BotCandidateInfo {
        BotCandidateInfo {
            account_id: 1,
            guid,
            race: 1,
            cls: 1,
            level,
            is_online: false,
            total_played_time: 100,
            map_id: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            o: 0.0,
            guild_id: 0,
            group_id: 0,
        }
    }

    #[test]
    fn worker_processes_tick_and_returns_commands() {
        let _g = TEST_LOCK.lock().unwrap();
        install_cfg();

        let mock = Arc::new(MockLoginWorld::new());
        mock.set_max_online_bot_count(10);
        mock.set_world_max_level(80);
        mock.set_candidates(vec![mk_candidate(42, 10)]);

        let handle = LoginWorkerHandle::spawn(mock.clone());
        handle.send_tick(Vec::new());
        let out = handle.recv_tick().expect("tick output");
        assert_eq!(out.commands.len(), 1);
        assert!(matches!(out.commands[0], QueuedCommand::Login(42)));

        // Dispatch — should call perform_login on the mock.
        handle.dispatch_commands(&out);
        drop(handle);
        let events = mock.events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockLoginEvent::PerformLogin(42))),
            "expected perform_login(42) event"
        );
    }

    #[test]
    fn worker_exits_on_shutdown_message() {
        let _g = TEST_LOCK.lock().unwrap();
        install_cfg();

        let mock = Arc::new(MockLoginWorld::new());
        mock.set_max_online_bot_count(5);
        mock.set_world_max_level(80);
        let handle = LoginWorkerHandle::spawn(mock);
        // Drop without sending a tick should still cleanly join.
        drop(handle);
    }

    #[test]
    fn worker_toggle_debug_emits_log_lines() {
        let _g = TEST_LOCK.lock().unwrap();
        install_cfg();

        let mock = Arc::new(MockLoginWorld::new());
        mock.set_max_online_bot_count(5);
        mock.set_world_max_level(80);
        mock.set_candidates(vec![mk_candidate(1, 10)]);

        let handle = LoginWorkerHandle::spawn(mock.clone());
        assert!(handle.toggle_debug());
        handle.send_tick(Vec::new());
        let _ = handle.recv_tick().expect("tick output");
        drop(handle);

        // With debug on, the tick should have produced at least one
        // `Debug` event.
        let events = mock.events();
        assert!(
            events.iter().any(|e| matches!(e, MockLoginEvent::Debug(_))),
            "expected a Debug event after toggling debug"
        );
    }
}
