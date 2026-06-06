//! `playerbot` — Rust AI module for `CMaNGOS` playerbots.
//!
//! Compiles to a static library (`libplayerbot_rs.a`) linked by the C++
//! wrapper. All public symbols are `extern "C"` and declared in
//! `cpp_wrapper/botffi.h`. The cross-FFI surface lives in `exports.rs`;
//! everything else is safe Rust.

#![deny(unsafe_code)]

pub mod bdi;
pub mod bot;
pub mod classes;
pub mod combat;
pub mod commands;
#[allow(unsafe_code)]
pub mod config;
pub mod data;
pub mod encounters;
pub mod engine;
pub mod factory;
pub mod goap;
#[allow(unsafe_code)]
pub mod itempool;
#[allow(unsafe_code)]
pub mod logging;
#[allow(unsafe_code)]
pub mod login;
pub mod manager;
pub mod noncombat;
pub mod npc_flags;
pub mod random;
pub mod random_mgr;
pub mod rtsc;
pub mod strategies;
pub mod travel;
pub mod world;

#[allow(unsafe_code)]
mod exports;
