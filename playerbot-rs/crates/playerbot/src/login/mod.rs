//! Playerbot login/logout queue — Rust port of `PlayerbotLoginMgr.cpp`.
//!
//! The queue is split into three layers:
//!
//! * [`state`] — pure data types and state-machine helpers on a single
//!   candidate (`PlayerLoginInfo`, `HolderState`, `LoginState`, `FillStep`,
//!   `LoginCriterionFailType`). No I/O, no global state.
//! * [`space`] — `LoginSpace` bookkeeping: per-tick snapshot of total
//!   available slots, per-class-race buckets, per-level buckets, and the
//!   real-player info list used by distance/map/group/guild gates.
//! * [`criteria`] — criterion name parsing (`"maxbots"`, `"range"`, …)
//!   plus the `MatchNoCriteria` evaluator that produces a
//!   `LoginCriterionFailType` for each candidate.
//! * [`manager`] — the top-level `LoginManager` that orchestrates a tick:
//!   load candidates, build the space, iterate attempts, enqueue
//!   login/logout commands, emit debug lines.
//! * [`worker`] — the Rust background thread that drives `manager` and
//!   communicates with the main tick loop via `mpsc::channel`.
//! * [`ffi`] — the `extern "C"` surface consumed by `LoginBridge.cpp`.

pub mod criteria;
pub mod manager;
pub mod space;
pub mod state;
pub mod worker;

#[allow(unsafe_code)]
pub mod ffi;

pub use criteria::{Criterion, LoginCriterionFailType, criteria_for_attempt, parse_criterion};
pub use manager::{LoginManager, QueuedCommand, TickOutput};
pub use space::{LoginSpace, RealPlayerSummary};
pub use state::{FillStep, LoginState, PlayerLoginInfo};
pub use worker::{LoginWorkerHandle, WorkerRequest};
