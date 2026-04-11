//! Guild initialization — port of `PlayerbotFactory::InitGuild`.
//!
//! C++ body (`playerbot/PlayerbotFactory.cpp::InitGuild`):
//!
//! ```text
//! if (bot->GetGuildId() && !bot->HasItemCount(5976, 1))
//!     StoreItem(5976, 1);                              // (1) tabard top-up
//!
//! if (bot->GetGuildId()) return;                       // (2) already guilded
//!
//! if (sPlayerbotAIConfig.randomBotGuilds.size()
//!         < sPlayerbotAIConfig.randomBotGuildCount)
//!     playerbot_random_factory_create_guilds();        // (3) ensure candidate pool
//!
//! std::vector<uint32> guilds;
//! for (auto id : sPlayerbotAIConfig.randomBotGuilds) { // (4) same-team filter
//!     Guild* guild = sGuildMgr.GetGuildById(id);
//!     if (!guild) continue;
//!     if (sObjectMgr.GetPlayerTeamByGUID(guild->GetLeaderGuid())
//!             != bot->GetTeam()) continue;
//!     guilds.push_back(id);
//! }
//! if (guilds.empty()) {
//!     sLog.outError("No random guilds available"); return;
//! }
//!
//! int index = urand(0, guilds.size() - 1);             // (5) random pick
//! uint32 guildId = guilds[index];
//! Guild* guild = sGuildMgr.GetGuildById(guildId);
//! if (!guild) { sLog.outError("Can't join random guild, ID: %u", guildId); return; }
//!
//! uint32 num = atoi(guild->GetGINFO().c_str());        // (6) capacity check
//! if ((num && guild->GetMemberSize() < num)
//!         || (!num && guild->GetMemberSize() < urand(10, 15))) {
//!     uint32 rankId = urand(GR_OFFICER, GR_INITIATE);
//!     guild->AddMember(bot->GetObjectGuid(), rankId);
//!     sLog.outDetail("Bot #%d %s:%d <%s>: Guild <%s> R: %s", ...);
//! }
//!
//! if (bot->GetGuildId() && bot->GetLevel() > 9         // (7) post-join tabard
//!         && urand(0, 4) && !bot->HasItemCount(5976, 1))
//!     StoreItem(5976, 1);
//! ```
//!
//! Steps map 1:1 to branches in [`init_guild`]. The `randomBotGuilds` list
//! used in step (4) now lives in the Rust random-factory singleton (see
//! `random::factory::FactoryState::random_bot_guilds`) — the legacy C++
//! mirror field has been unused since Phase G. We read it via
//! `random::ffi::random_bot_guilds` and read the per-guild facts (leader
//! team, member count, GINFO cap, name) through the four new
//! `factory_*_guild_*` FFI entry points in `cmangos::World`.
//!
//! The step-(3) create-pass gate mirrors the PB2 predicate exactly:
//! "current tracked guild count < configured target". When it fires we
//! call `playerbot_random_factory_create_guilds()`, which in turn runs
//! `random::factory::create_random_guilds` and pushes fresh guild ids
//! back into `FactoryState::random_bot_guilds`. The gate uses a fresh
//! `random_bot_guilds_count()` read under the singleton lock rather than
//! the length of the snapshot we fetch for step (4), so a concurrent
//! create pass racing us can't trick us into skipping the refill.

use super::FactoryTransaction;
use crate::log_error;
use cmangos::ItemId;

/// Tabard of the guild — the item PB2's `PlayerbotFactory::InitGuild`
/// hands out both on tabard top-up and post-join. Matches the item id
/// hard-coded in the C++ source.
const GUILD_TABARD_ITEM_ID: ItemId = ItemId(5976);

/// Rank range used by the random post-join: `urand(GR_OFFICER,
/// GR_INITIATE)` in PB2, inclusive on both ends. Guild ranks in
/// `CMaNGOS` (`Guilds/Guild.h`): `GR_GUILDMASTER=0`, `GR_OFFICER=1`,
/// `GR_VETERAN=2`, `GR_MEMBER=3`, `GR_INITIATE=4`.
const GR_OFFICER: u32 = 1;
const GR_INITIATE: u32 = 4;

/// Fallback capacity cap when `Guild::GetGINFO()` does not parse as a
/// positive integer. PB2 rolls `urand(10, 15)` in that branch so a
/// leaderless guild doesn't reject every applicant.
const GINFO_FALLBACK_CAP_MIN: u32 = 10;
const GINFO_FALLBACK_CAP_MAX: u32 = 15;

/// Minimum bot level for the post-join tabard hand-out. PB2 uses
/// `bot->GetLevel() > 9` which excludes levels 1..=9.
const POST_JOIN_TABARD_MIN_LEVEL: u32 = 10;

/// Odds for the post-join tabard gate. PB2 writes `urand(0, 4)` and
/// treats *any* non-zero result as "hand out a tabard", so the tabard
/// is given 4/5 of the time once the other gates pass. We mirror that
/// semantic with `random_u32(0, 4) != 0`.
const POST_JOIN_TABARD_DICE: u32 = 4;

/// Port of `PlayerbotFactory::InitGuild`. Runs the full pre-/post-join
/// flow: tabard top-up for already-guilded bots, same-team filtered
/// candidate pick, capacity-gated join attempt, and post-join tabard
/// hand-out. Silent no-op when the random-factory singleton is
/// uninitialised or there are no same-team candidates.
pub fn init_guild(tx: &mut FactoryTransaction<'_>) {
    // Pull team + level + initial guild id from one snapshot so the
    // per-function call count matches the C++ source (PB2 reads these
    // via direct Player getters; a single snapshot is the cheaper
    // equivalent).
    let snap = tx.get_snapshot();
    let bot_team = snap.self_.team;
    let bot_level = u32::from(snap.self_.level);
    let initial_guild_id = snap.guild_id;

    // (1) Tabard top-up for already-guilded bots.
    if initial_guild_id != 0 && !tx.has_item(GUILD_TABARD_ITEM_ID) {
        tx.inventory_add_item(GUILD_TABARD_ITEM_ID, 1);
    }

    // (2) Already guilded → done.
    if initial_guild_id != 0 {
        return;
    }

    // (3) Kick off a create pass when the tracked pool is below target.
    // We ask the singleton for the live count under a single lock
    // acquisition so we can't race a concurrent create pass.
    let cfg = crate::config::get();
    if crate::random::ffi::random_bot_guilds_count() < cfg.random_bot_guild_count {
        crate::random::ffi::playerbot_random_factory_create_guilds();
    }

    // (4) Same-team filter. Pull a fresh snapshot of the tracked guild
    // ids and drop any whose leader is on the opposing team.
    let candidate_ids = crate::random::ffi::random_bot_guilds();
    let mut same_team: Vec<u32> = Vec::with_capacity(candidate_ids.len());
    for guild_id in &candidate_ids {
        let Some(summary) = tx.factory_query_guild_summary(*guild_id) else {
            continue;
        };
        if summary.leader_team != bot_team {
            continue;
        }
        same_team.push(*guild_id);
    }
    if same_team.is_empty() {
        log_error!("No random guilds available");
        return;
    }

    // (5) Random pick.
    let pick_idx = tx.random_u32(0, same_team.len() as u32 - 1) as usize;
    let pick_idx = pick_idx.min(same_team.len() - 1);
    let guild_id = same_team[pick_idx];
    let Some(summary) = tx.factory_query_guild_summary(guild_id) else {
        // Racy: the guild existed a few lines ago but disappeared.
        // Mirrors the PB2 log + early return when `GetGuildById` fails
        // on the second pass.
        log_error!("Can't join random guild, ID: {}", guild_id);
        return;
    };

    // (6) Capacity gate + join. PB2 parses the guild's GINFO string as
    // a soft cap; when the string is not numeric (`atoi -> 0`) we fall
    // back to a rolling `urand(10, 15)` cap.
    let cap_ok = if summary.max_members_hint != 0 {
        summary.member_size < summary.max_members_hint
    } else {
        summary.member_size < tx.random_u32(GINFO_FALLBACK_CAP_MIN, GINFO_FALLBACK_CAP_MAX)
    };
    if cap_ok {
        let rank_id = tx.random_u32(GR_OFFICER, GR_INITIATE);
        if tx.factory_guild_add_member(guild_id, rank_id) {
            let rank_name = tx
                .factory_get_guild_rank_name(guild_id, rank_id)
                .unwrap_or_else(|| format!("R{rank_id}"));
            // PB2's original log also includes `bot->GetGUIDLow()` and
            // `bot->GetName()`. Neither is currently exposed through
            // the `cmangos::World` trait, so we log the bot's team +
            // level instead and keep guild/rank for operator context.
            crate::log_info!(
                "Bot {}:{}: Guild <{}> R: {}",
                if bot_team == 0 { "A" } else { "H" },
                bot_level,
                summary.name,
                rank_name,
            );
        }
    }

    // (7) Post-join tabard. The join may or may not have fired above,
    // so re-read the guild id from the FFI rather than trusting the
    // initial snapshot.
    if tx.factory_bot_guild_id() != 0
        && bot_level > (POST_JOIN_TABARD_MIN_LEVEL - 1)
        && tx.random_u32(0, POST_JOIN_TABARD_DICE) != 0
        && !tx.has_item(GUILD_TABARD_ITEM_ID)
    {
        tx.inventory_add_item(GUILD_TABARD_ITEM_ID, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{BotUnitSnapshot, BotWorldSnapshot, GuildSummary, MockEvent, MockWorld, World};

    /// Assemble the minimal snapshot `init_guild` reads: team, level,
    /// and the initial guild id. All other fields default to zero.
    fn snapshot(team: u8, level: u8, guild_id: u32) -> BotWorldSnapshot {
        let self_ = BotUnitSnapshot {
            team,
            level,
            ..Default::default()
        };
        BotWorldSnapshot {
            self_,
            guild_id,
            ..Default::default()
        }
    }

    /// Build a `MockWorld` configured for `init_guild` tests: the
    /// snapshot is pre-assembled by the caller and the RNG is fixed so
    /// both the random pick and the post-join tabard dice roll are
    /// deterministic.
    fn make_world(snap: &BotWorldSnapshot, rand_fixed: u32) -> MockWorld {
        MockWorld::builder()
            .world_snap(*snap)
            .random_default(rand_fixed)
            .build()
    }

    /// Convenience: run `init_guild` and return the recorded events.
    fn run(world: &mut MockWorld) -> Vec<MockEvent> {
        with_tx(world, init_guild);
        world.events()
    }

    #[test]
    fn already_guilded_without_tabard_stores_tabard_and_returns() {
        // Bot is already in guild 42 and is missing the tabard, so the
        // pre-guard should hand out one tabard and return without any
        // further joins or dice rolls.
        let world = make_world(&snapshot(0, 30, 42), 0);
        let mut world = world;
        let events = run(&mut world);

        let adds: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, MockEvent::InventoryAddItem { item, .. } if item.0 == 5976))
            .collect();
        assert_eq!(adds.len(), 1, "already-guilded bot should get exactly one tabard");

        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MockEvent::GuildAddMember { .. })),
            "already-guilded bot should not attempt a join"
        );
    }

    #[test]
    fn already_guilded_with_tabard_is_noop() {
        let world = MockWorld::builder()
            .world_snap(snapshot(1, 60, 99))
            .random_default(0)
            .build();
        let mut world = world;
        // Seed the bot's bag with a tabard so `has_item` returns true.
        world.with_state(|s| {
            s.bag_items.insert(5976, 1);
        });

        let events = run(&mut world);
        assert!(
            events.is_empty(),
            "already-guilded bot with a tabard should do nothing, got: {events:?}"
        );
    }

    #[test]
    fn ungulided_with_empty_candidate_list_logs_error() {
        // Empty `random_bot_guilds` singleton → no candidates after
        // filtering → early-return on the error path. We observe this
        // indirectly: no join events, no item adds.
        let mut world = make_world(&snapshot(0, 30, 0), 0);
        let events = run(&mut world);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MockEvent::GuildAddMember { .. })),
            "empty candidate list should not produce a join"
        );
        assert!(
            !events.iter().any(
                |e| matches!(e, MockEvent::InventoryAddItem { item, .. } if item.0 == 5976)
            ),
            "empty candidate list should not produce a tabard add"
        );
    }

    /// Helper: pull the number of tabard adds out of the event log.
    fn tabard_adds(events: &[MockEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, MockEvent::InventoryAddItem { item, .. } if item.0 == 5976))
            .count()
    }

    #[test]
    fn gate_constants_match_pb2() {
        // Guard against accidental re-numbering. The C++ source
        // hard-codes these values in PlayerbotFactory.cpp::InitGuild.
        assert_eq!(GUILD_TABARD_ITEM_ID.0, 5976);
        assert_eq!(GR_OFFICER, 1);
        assert_eq!(GR_INITIATE, 4);
        assert_eq!(GINFO_FALLBACK_CAP_MIN, 10);
        assert_eq!(GINFO_FALLBACK_CAP_MAX, 15);
        assert_eq!(POST_JOIN_TABARD_MIN_LEVEL, 10);
        assert_eq!(POST_JOIN_TABARD_DICE, 4);
    }

    #[test]
    fn already_guilded_eagerly_tops_up_tabard_once_per_call() {
        // Re-running `init_guild` on a guilded bot that now owns a
        // tabard must be a no-op — the tabard hand-out is strictly
        // "give one if missing".
        let snap = snapshot(0, 40, 77);
        let mut world = make_world(&snap, 0);
        let first_events = run(&mut world);
        assert_eq!(tabard_adds(&first_events), 1, "first call must hand out a tabard");

        // Simulate the tabard entering the bot's bags.
        world.with_state(|s| {
            s.bag_items.insert(5976, 1);
            s.events.clear();
        });

        let second_events = run(&mut world);
        assert_eq!(tabard_adds(&second_events), 0, "second call with tabard must be idle");
        assert!(
            !second_events
                .iter()
                .any(|e| matches!(e, MockEvent::GuildAddMember { .. })),
            "already-guilded path must never hit the join branch"
        );
    }

    #[test]
    fn snapshot_helper_produces_reachable_state() {
        // Sanity-check the test helper: the snapshot it builds is what
        // `init_guild` consumes, and the `already_guilded_*` tests rely
        // on the team / level / guild_id wiring being exact.
        let s = snapshot(1, 42, 77);
        assert_eq!(s.self_.team, 1);
        assert_eq!(s.self_.level, 42);
        assert_eq!(s.guild_id, 77);
    }

    #[test]
    fn mock_guild_summary_round_trips_through_trait() {
        // The mock layer must honour `factory_query_guild_summary` for
        // the guild list the test seeds — this is the glue the
        // "pick + join" path depends on.
        let world = MockWorld::builder()
            .guild_summary(
                42,
                GuildSummary {
                    leader_team: 0,
                    member_size: 3,
                    max_members_hint: 50,
                    name: "Test Guild".into(),
                },
            )
            .guild_rank_name(42, 1, "Officer")
            .build();

        let summary = world.factory_query_guild_summary(42).expect("seeded");
        assert_eq!(summary.member_size, 3);
        assert_eq!(summary.max_members_hint, 50);
        assert_eq!(summary.name, "Test Guild");

        assert_eq!(
            world.factory_get_guild_rank_name(42, 1).as_deref(),
            Some("Officer")
        );
        assert!(world.factory_query_guild_summary(999).is_none());
    }

    #[test]
    fn mock_guild_add_member_flips_bot_guild_id_and_records_event() {
        // The `factory_guild_add_member` path must both record an
        // event and synchronously update `bot_guild_id` so the
        // post-join re-check in `init_guild` sees the join.
        let world = MockWorld::builder()
            .guild_summary(
                42,
                GuildSummary {
                    leader_team: 0,
                    member_size: 3,
                    max_members_hint: 50,
                    name: "Test Guild".into(),
                },
            )
            .build();

        assert!(world.factory_guild_add_member(42, 2));
        assert_eq!(world.factory_bot_guild_id(), 42);

        let events = world.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, MockEvent::GuildAddMember { guild_id: 42, rank: 2 })));

        // An unknown guild id must still be rejected.
        assert!(!world.factory_guild_add_member(999, 1));
    }
}
