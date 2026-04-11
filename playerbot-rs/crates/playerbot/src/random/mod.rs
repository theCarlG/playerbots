//! Random playerbot factory — Rust port of `RandomPlayerbotFactory.cpp`.
//!
//! The module replaces the C++ `RandomPlayerbotFactory` class with a set
//! of pure Rust helpers plus a thin orchestration layer that calls into
//! `CMaNGOS` through the [`RandomFactoryWorld`](
//! ::cmangos::random_factory_world::RandomFactoryWorld) trait.
//!
//! Layout:
//!
//! * [`races`] — compile-time class/race availability matrix, the
//!   `NameRaceAndGender` enum, and the `combine_race_and_gender`
//!   helper. Pure const data.
//! * [`selection`] — probability-weighted class/race selection
//!   (`get_random_class` / `get_random_race`) plus the `name_postfix`
//!   bijective-base-26 helper used by the name-pool expansion pass.
//!   Takes an explicit [`selection::FactoryRng`] so tests drive
//!   deterministically.
//! * [`factory`] — the three orchestration entry points
//!   (`create_random_bots`, `create_random_guilds`,
//!   `create_random_arena_teams`) and the state container the bridge
//!   uses to track runtime data (`random_bot_accounts`,
//!   `random_bot_guilds`, `random_bot_arena_teams`).
//! * [`ffi`] — the `extern "C"` surface consumed by
//!   `cpp_wrapper/RandomFactoryBridge.cpp`.

pub mod factory;
pub mod races;
pub mod selection;

#[allow(unsafe_code)]
pub mod ffi;

pub use factory::{
    FactoryState, create_random_arena_teams, create_random_bots, create_random_guilds,
};
pub use races::{
    NameRaceAndGender, availability, combine_race_and_gender, first_available_race,
    is_available_race,
};
pub use selection::{FactoryRng, SequentialRng, get_random_class, get_random_race, name_postfix};
