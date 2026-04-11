//! `PrintStats` port — builds the multi-line chat output the console
//! `stats` command emits.
//!
//! The C++ original at `RandomPlayerbotMgr.cpp:3287` walks the live
//! `players` map once per call and tallies:
//!
//! * Level distribution (ten buckets of 10 levels each, split by faction).
//! * Race / class histograms.
//! * Role split (tank/heal/dps), derived from `Player::getClass()`.
//! * Status flags: active, moving, on taxi, on mount, in combat, dead, AFK.
//! * Per-bot event readiness counters (`randomize`/teleport/`change_strategy`).
//!
//! The Rust port defers the per-bot crawl to the trait
//! ([`RandomMgrWorld::query_bot_stats`]). The bridge walks the
//! `players` map on the main thread and flattens every row into a
//! [`BotStatsSnapshotRow`]. `print_stats` then folds the rows and the
//! live [`EventCache`] into the exact same line output the C++ version
//! produced — callers can pipe the result into `world.log_info` and
//! forward it to the requesting session.

use cmangos::{BotStatsFlags, BotStatsSnapshotRow, RandomMgrWorld};

use super::commands::CommandOutput;
use super::events::EventCache;

/// Races mapped to faction 0 (Alliance). Mirrors `IsAlliance(race)` in
/// the C++ `Player::TeamForRace` helper, reduced to the subset that
/// matters for the stats split. Race ids outside this set fall back to
/// Horde.
const ALLIANCE_RACES: &[u8] = &[1, 3, 4, 7, 11];

/// Build the stats message lines. `rows` is the per-bot snapshot the
/// bridge collected on the main thread. `events` is consulted for
/// `randomize` / `teleport` / `change_strategy` event readiness.
///
/// The output is written into a fresh [`CommandOutput`] — each line
/// matches one `sLog.outString(...)` call in the legacy C++ version,
/// so the FFI layer can unconditionally `log_info` every entry.
pub fn print_stats(
    rows: &[BotStatsSnapshotRow],
    events: &mut EventCache,
    world: &dyn RandomMgrWorld,
    now_epoch_s: u32,
    max_level: u32,
) -> CommandOutput {
    let mut out = CommandOutput::new();
    out.push(format!("{} Random Bots online", rows.len()));

    // Faction-split level histogram (10 buckets of 10 levels each).
    let mut alliance = [0u32; 10];
    let mut horde = [0u32; 10];

    // Per-race / per-class histograms.
    let mut per_race: [u32; 256] = [0; 256];
    let mut per_class: [u32; 256] = [0; 256];

    // Flag counters — mirrors the C++ locals verbatim.
    let mut dps = 0u32;
    let mut heal = 0u32;
    let mut tank = 0u32;
    let mut active = 0u32;
    let mut randomize_ready = 0u32;
    let mut teleport_ready = 0u32;
    let mut change_strategy_ready = 0u32;
    let mut dead = 0u32;
    let mut combat = 0u32;
    let mut taxi = 0u32;
    let mut moving = 0u32;
    let mut mounted = 0u32;
    let mut afk = 0u32;

    for row in rows {
        let bucket = (row.level as usize / 10).min(9);
        if is_alliance_race(row.race) {
            alliance[bucket] = alliance[bucket].saturating_add(1);
        } else {
            horde[bucket] = horde[bucket].saturating_add(1);
        }

        per_race[row.race as usize] += 1;
        per_class[row.class_id as usize] += 1;

        if row.flags.contains(BotStatsFlags::ACTIVE) {
            active += 1;
        }
        if row.flags.contains(BotStatsFlags::DEAD) {
            dead += 1;
        }
        if row.flags.contains(BotStatsFlags::COMBAT) {
            combat += 1;
        }
        if row.flags.contains(BotStatsFlags::TAXI) {
            taxi += 1;
        }
        if row.flags.contains(BotStatsFlags::MOVING) {
            moving += 1;
        }
        if row.flags.contains(BotStatsFlags::MOUNTED) {
            mounted += 1;
        }
        if row.flags.contains(BotStatsFlags::AFK) {
            afk += 1;
        }

        // The C++ `!GetEventValue(...)` pattern counts bots that are
        // *ready* for the next randomise/teleport/change_strategy pass.
        // TTL = 0 means "event expired or not scheduled".
        if events.get_value(row.guid, "randomize", now_epoch_s, world) == 0 {
            randomize_ready += 1;
        }
        if events.get_value(row.guid, "teleport", now_epoch_s, world) == 0 {
            teleport_ready += 1;
        }
        if events.get_value(row.guid, "change_strategy", now_epoch_s, world) == 0 {
            change_strategy_ready += 1;
        }

        match row.class_id {
            // priest, druid, shaman → heal
            5 | 11 | 7 => heal += 1,
            // paladin, warrior, death knight → tank
            2 | 1 | 6 => tank += 1,
            // everyone else → dps
            _ => dps += 1,
        }
    }

    // Level distribution.
    out.push("Bots level:".to_string());
    for i in 0..10u32 {
        let a = alliance[i as usize];
        let h = horde[i as usize];
        if a == 0 && h == 0 {
            continue;
        }
        let mut from = i * 10;
        let to = (from + 9).min(max_level.saturating_sub(1));
        if from == 0 {
            from = 1;
        }
        out.push(format!(
            "    {from}..{to}: {a} alliance, {h} horde"
        ));
    }

    // Race histogram.
    out.push("Bots race:".to_string());
    for (race, count) in per_race.iter().enumerate().skip(1) {
        if *count > 0 {
            out.push(format!("    Race {race}: {count}"));
        }
    }

    // Class histogram.
    out.push("Bots class:".to_string());
    for (cls, count) in per_class.iter().enumerate().skip(1) {
        if *count > 0 {
            out.push(format!("    Class {cls}: {count}"));
        }
    }

    // Role split.
    out.push("Bots role:".to_string());
    out.push(format!("    tank: {tank}, heal: {heal}, dps: {dps}"));

    // Status block.
    out.push("Bots status:".to_string());
    out.push(format!("    Active: {active}"));
    out.push(format!("    Moving: {moving}"));
    out.push(format!("    On taxi: {taxi}"));
    out.push(format!("    On mount: {mounted}"));
    out.push(format!("    In combat: {combat}"));
    out.push(format!("    Dead: {dead}"));
    out.push(format!("    AFK: {afk}"));
    // Event readiness — kept despite the C++ version commenting these
    // out, since the Rust side has cheap access to the event cache.
    out.push(format!(
        "    Ready to randomize: {randomize_ready}, teleport: {teleport_ready}, change_strategy: {change_strategy_ready}"
    ));

    out
}

/// Returns `true` if the given race id is an Alliance race. Matches
/// the subset `IsAlliance(race)` returns on `CMaNGOS` Classic+TBC+WotLK.
#[must_use]
pub fn is_alliance_race(race: u8) -> bool {
    ALLIANCE_RACES.contains(&race)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockRandomMgrWorld;

    fn row(
        guid: u32,
        race: u8,
        class_id: u8,
        level: u32,
        flags: BotStatsFlags,
    ) -> BotStatsSnapshotRow {
        BotStatsSnapshotRow {
            guid,
            race,
            class_id,
            team: if is_alliance_race(race) { 0 } else { 1 },
            level,
            flags,
        }
    }

    #[test]
    fn empty_snapshot_still_prints_header_and_zeros() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        let out = print_stats(&[], &mut events, &world, 1000, 60);
        assert_eq!(out.messages[0], "0 Random Bots online");
        // Role line is always present.
        assert!(out.messages.iter().any(|m| m.contains("tank: 0")));
    }

    #[test]
    fn level_buckets_split_by_faction() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        let rows = vec![
            // Human warrior L20 — Alliance bucket 2.
            row(1, 1, 1, 20, BotStatsFlags::empty()),
            // Orc rogue L25 — Horde bucket 2.
            row(2, 2, 4, 25, BotStatsFlags::empty()),
            // Night elf druid L40 — Alliance bucket 4.
            row(3, 4, 11, 40, BotStatsFlags::empty()),
        ];
        let out = print_stats(&rows, &mut events, &world, 1000, 60);

        let joined = out.messages.join("\n");
        assert!(joined.contains("20..29: 1 alliance, 1 horde"));
        assert!(joined.contains("40..49: 1 alliance, 0 horde"));
    }

    #[test]
    fn role_counts_use_class_mapping() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        let rows = vec![
            // Warrior → tank.
            row(1, 1, 1, 20, BotStatsFlags::empty()),
            // Priest → heal.
            row(2, 1, 5, 20, BotStatsFlags::empty()),
            // Mage → dps.
            row(3, 1, 8, 20, BotStatsFlags::empty()),
        ];
        let out = print_stats(&rows, &mut events, &world, 1000, 60);
        assert!(out.messages.iter().any(|m| m.contains("tank: 1, heal: 1, dps: 1")));
    }

    #[test]
    fn status_flags_are_counted() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        let rows = vec![
            row(1, 1, 1, 20, BotStatsFlags::ACTIVE | BotStatsFlags::COMBAT),
            row(
                2,
                1,
                1,
                20,
                BotStatsFlags::ACTIVE | BotStatsFlags::DEAD | BotStatsFlags::AFK,
            ),
            row(3, 1, 1, 20, BotStatsFlags::ACTIVE | BotStatsFlags::TAXI),
        ];
        let out = print_stats(&rows, &mut events, &world, 1000, 60);
        let joined = out.messages.join("\n");
        assert!(joined.contains("Active: 3"));
        assert!(joined.contains("Dead: 1"));
        assert!(joined.contains("In combat: 1"));
        assert!(joined.contains("AFK: 1"));
        assert!(joined.contains("On taxi: 1"));
    }

    #[test]
    fn event_readiness_ignores_bots_with_active_events() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        // Bot 1 has a pending randomize event → not ready.
        events.set_value(1, "randomize", 1, 10_000, "", 0, &world);
        let rows = vec![
            row(1, 1, 1, 20, BotStatsFlags::empty()),
            row(2, 1, 1, 20, BotStatsFlags::empty()),
        ];
        let out = print_stats(&rows, &mut events, &world, 1000, 60);
        // Bot 2 has nothing scheduled → ready for all three.
        // Bot 1 is blocked on randomize → only teleport + change_strategy.
        let joined = out.messages.join("\n");
        assert!(joined.contains("Ready to randomize: 1"));
        assert!(joined.contains("teleport: 2"));
        assert!(joined.contains("change_strategy: 2"));
    }

    #[test]
    fn horde_race_goes_to_horde_bucket() {
        assert!(!is_alliance_race(2)); // orc
        assert!(is_alliance_race(1)); // human
        assert!(is_alliance_race(3)); // dwarf
        assert!(is_alliance_race(4)); // night elf
        assert!(is_alliance_race(7)); // gnome
        assert!(is_alliance_race(11)); // draenei
        assert!(!is_alliance_race(5)); // undead
        assert!(!is_alliance_race(6)); // tauren
        assert!(!is_alliance_race(8)); // troll
        assert!(!is_alliance_race(10)); // blood elf
    }
}
