//! Arena team initialization — port of `PlayerbotFactory::InitArenaTeam`.
//!
//! C++ body:
//!
//! ```text
//! #ifndef MANGOSBOT_ZERO
//! void PlayerbotFactory::InitArenaTeam() {
//!     if (!sPlayerbotAIConfig.IsInRandomAccountList(
//!             bot->GetSession()->GetAccountId()))
//!         return;
//!     if (sPlayerbotAIConfig.randomBotArenaTeams.size()
//!             < sPlayerbotAIConfig.randomBotArenaTeamCount)
//!         playerbot_random_factory_create_arena_teams();
//! }
//! #endif
//! ```
//!
//! Two gates: (1) the bot's owning account is in the live random-bot account
//! list, and (2) the current tracked arena-team count is below the configured
//! target. When either gate fails the function is a silent no-op.
//!
//! The runtime list used by gate (1) (the C++ mirror field
//! `sPlayerbotAIConfig.randomBotAccounts`) is never written to after the
//! migration stripped the C++ random-factory code; the authoritative list
//! now lives in the Rust random-factory singleton (`FactoryState::
//! random_bot_accounts`). Gate (2) similarly reads from
//! `FactoryState::random_bot_arena_teams`. Both are accessed via helpers
//! in `crate::random::ffi` that acquire the singleton lock once per call.
//!
//! Gated on `feature = "tbc"` or `feature = "wotlk"` — Classic has no arena
//! teams and the C++ call site was already `#ifndef MANGOSBOT_ZERO`-gated.
//! On Classic the module exposes a no-op `init_arena_team` so the FFI entry
//! point always links.

use super::FactoryTransaction;

/// Pure policy gate — does the bot need a new arena-team creation pass?
///
/// Extracted from `init_arena_team` so the gate logic can be unit-tested
/// without initialising the random-factory singleton. Only used by the
/// local test module; the real orchestration in [`init_arena_team`] calls
/// [`crate::random::ffi::should_create_arena_teams`] directly so it can
/// read the live singleton state under a single lock.
#[cfg(all(test, any(feature = "tbc", feature = "wotlk")))]
pub(crate) fn should_init_arena_team(
    account_id: u32,
    random_accounts: &[u32],
    current_arena_teams: u32,
    target_arena_teams: u32,
) -> bool {
    random_accounts.contains(&account_id) && current_arena_teams < target_arena_teams
}

/// Port of `PlayerbotFactory::InitArenaTeam` for TBC/WotLK.
///
/// Reads the bot's owning account id via the factory transaction, consults
/// the random-factory singleton for the live random-bot account and arena-
/// team lists, and — when both gates pass — forwards to
/// `playerbot_random_factory_create_arena_teams()`.
#[cfg(any(feature = "tbc", feature = "wotlk"))]
pub fn init_arena_team(tx: &mut FactoryTransaction<'_>) {
    let account_id = tx.bot_get_account_id();
    let cfg = crate::config::get();
    if !crate::random::ffi::should_create_arena_teams(account_id, cfg.random_bot_arena_team_count) {
        return;
    }
    crate::random::ffi::playerbot_random_factory_create_arena_teams();
}

/// Classic stub — vanilla has no arena teams. Kept so the FFI entry point
/// always links.
#[cfg(not(any(feature = "tbc", feature = "wotlk")))]
pub fn init_arena_team(_tx: &mut FactoryTransaction<'_>) {}

#[cfg(all(test, any(feature = "tbc", feature = "wotlk")))]
mod tests {
    use super::*;

    #[test]
    fn triggers_when_account_listed_and_count_below_target() {
        assert!(should_init_arena_team(42, &[41, 42, 43], 5, 10));
    }

    #[test]
    fn skips_when_account_not_in_random_list() {
        // The bot is not a random bot — gate (1) fails.
        assert!(!should_init_arena_team(99, &[41, 42, 43], 5, 10));
    }

    #[test]
    fn skips_when_target_already_reached() {
        // Random bot, but the factory already has enough arena teams.
        assert!(!should_init_arena_team(42, &[42], 10, 10));
    }

    #[test]
    fn skips_when_target_exceeded() {
        // Defensive: count > target still counts as "already reached".
        assert!(!should_init_arena_team(42, &[42], 11, 10));
    }

    #[test]
    fn skips_when_target_is_zero() {
        // Configured target of 0 means "no arena teams wanted" — should
        // always skip even for a listed account with an empty team list.
        assert!(!should_init_arena_team(42, &[42], 0, 0));
    }

    #[test]
    fn skips_when_random_accounts_empty() {
        // Empty list → nothing matches → gate (1) fails.
        assert!(!should_init_arena_team(42, &[], 0, 10));
    }
}
