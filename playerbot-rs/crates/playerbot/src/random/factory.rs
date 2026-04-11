//! Orchestration entry points — Rust ports of
//! `playerbot/RandomPlayerbotFactory.cpp`'s three public `Create*`
//! methods.
//!
//! Each function takes an explicit `&dyn RandomFactoryWorld` so the C
//! bridge uses [`VtableRandomFactoryWorld`](
//! ::cmangos::random_factory_world::VtableRandomFactoryWorld) and the
//! unit tests use [`MockRandomFactoryWorld`](
//! ::cmangos::random_factory_world::MockRandomFactoryWorld). The
//! runtime tracking lists (`random_bot_accounts`, `random_bot_guilds`,
//! `random_bot_arena_teams`) plus the two guild / arena-team deletion
//! flags that the legacy C++ `RandomPlayerbotMgr` held live on
//! [`FactoryState`].
//!
//! The port is deliberately close to the C++ source — names, loop
//! shapes, and log messages are preserved so behaviour is easy to
//! diff against the original.

use std::collections::{BTreeMap, HashMap};

use cmangos::{CreateParams, NamePoolRow, RandomFactoryWorld};

use crate::config::typed::{BotConfig, MAX_CLASSES};
use crate::random::races::{
    GENDER_FEMALE, GENDER_MALE, NameRaceAndGender, combine_race_and_gender, is_available_race,
};
#[cfg(not(feature = "wotlk"))]
use crate::random::races::{RACE_NIGHTELF, RACE_TAUREN, RACE_UNDEAD};
use crate::random::selection::{FactoryRng, get_random_class, get_random_race, name_postfix};

/// `DEFAULT_MAX_LEVEL` on the current expansion — 80 on WotLK, 70 on
/// TBC, 60 on Classic. Matches the core `DEFAULT_MAX_LEVEL` macro that
/// the legacy arena-team code used as the captain level gate.
///
/// Only the arena-team path reads this, so on Classic (where the path
/// is a stub) the constant is only used by tests.
#[cfg(feature = "wotlk")]
const DEFAULT_MAX_LEVEL: u8 = 80;
#[cfg(all(feature = "tbc", not(feature = "wotlk")))]
const DEFAULT_MAX_LEVEL: u8 = 70;
#[cfg(all(test, not(any(feature = "tbc", feature = "wotlk"))))]
const DEFAULT_MAX_LEVEL: u8 = 60;

/// Per-account character cap — 10 on WotLK (death knight extra slot),
/// 9 on Classic / TBC.
#[cfg(feature = "wotlk")]
const ACCOUNT_CHAR_LIMIT: u32 = 10;
#[cfg(not(feature = "wotlk"))]
const ACCOUNT_CHAR_LIMIT: u32 = 9;

/// `MAX_EXPANSION` on TBC/WotLK, 0 on Classic. Passed to
/// `create_account` so the new accounts have the right expansion flag.
#[cfg(feature = "wotlk")]
const MAX_EXPANSION: u8 = 2;
#[cfg(all(feature = "tbc", not(feature = "wotlk")))]
const MAX_EXPANSION: u8 = 1;
#[cfg(not(any(feature = "tbc", feature = "wotlk")))]
const MAX_EXPANSION: u8 = 0;

/// Runtime state the C++ `RandomPlayerbotMgr` / `PlayerbotAIConfig`
/// used to track between calls: the list of random bot account ids,
/// the list of random bot guild ids, the list of random bot arena
/// team ids, and the two "already ran the delete pass" latches.
///
/// Held as a `Mutex<Option<FactoryState>>` singleton by the FFI
/// module; unit tests construct a local instance.
#[derive(Debug, Default, Clone)]
pub struct FactoryState {
    pub random_bot_accounts: Vec<u32>,
    pub random_bot_guilds: Vec<u32>,
    pub random_bot_arena_teams: Vec<u32>,
    pub guilds_deleted: bool,
    pub arena_teams_deleted: bool,
}

impl FactoryState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Thin adapter implementing [`FactoryRng`] on top of a
/// [`RandomFactoryWorld`] so [`get_random_class`] and [`get_random_race`]
/// can use the production `urand_range` callback.
struct WorldRng<'a, W: RandomFactoryWorld + ?Sized> {
    world: &'a W,
}

impl<'a, W: RandomFactoryWorld + ?Sized> FactoryRng for WorldRng<'a, W> {
    fn urand_range(&mut self, min: u32, max: u32) -> u32 {
        self.world.urand_range(min, max)
    }
}

/// Port of `RandomPlayerbotFactory::CreateRandomBots`.
///
/// Deletes random bot characters (or whole accounts), creates missing
/// random bot accounts, rebuilds the name pool, and fills each account
/// with the configured class/race distribution.
///
/// `state` is updated in place: on success the function appends newly-
/// minted account ids to `state.random_bot_accounts`.
pub fn create_random_bots(
    cfg: &BotConfig,
    world: &dyn RandomFactoryWorld,
    state: &mut FactoryState,
) {
    // ── 1. Check scheduled `bot_delete` event ─────────────────────────
    let delete_event = world.query_bot_delete_event();
    let del_accs = delete_event.scheduled;
    let del_friends = delete_event.delete_friends;

    // ── 2. Delete pass ─────────────────────────────────────────────────
    if cfg.delete_random_bot_accounts || del_accs {
        delete_random_bots(cfg, world, del_friends);
    }

    // ── 3. Auto-create disabled — just record existing accounts ───────
    if !cfg.random_bot_auto_create {
        for account_number in 0..cfg.random_bot_account_count {
            let account_name = format!("{}{}", cfg.random_bot_account_prefix, account_number);
            let account_id = world.query_account_id_by_name(&account_name);
            if account_id != 0 {
                state.random_bot_accounts.push(account_id);
            }
        }
        return;
    }

    // ── 4. Account creation ────────────────────────────────────────────
    world.log_debug("Creating random bot accounts...");
    for account_number in 0..cfg.random_bot_account_count {
        let account_name = format!("{}{}", cfg.random_bot_account_prefix, account_number);
        if world.query_account_id_by_name(&account_name) != 0 {
            continue;
        }
        let password = if cfg.random_bot_random_password {
            generate_random_password(world)
        } else {
            account_name.clone()
        };
        world.create_account(&account_name, &password, MAX_EXPANSION);
    }

    // ── 5. Character count target ──────────────────────────────────────
    let total_char_count = cfg.random_bot_account_count * ACCOUNT_CHAR_LIMIT;

    // ── 6. Load name pool ──────────────────────────────────────────────
    world.log_debug("Loading available names...");
    let name_pool_rows = world.query_name_pool();
    let (mut free_names, mut all_names, mut used) = build_name_pools(name_pool_rows);

    // Fallback: no rows for DwarfMale means `ai_playerbot_names` has
    // not been loaded; reseed `used` from existing character names and
    // synthesise the pool via postfix expansion over the generic row.
    if !all_names.contains_key(&NameRaceAndGender::DwarfMale) {
        world.log_debug(
            "The name database has not been updated. Run ai_playerbot_names.sql to update.",
        );
        for row in world.query_existing_character_names() {
            used.insert(row.name, false);
        }
        synthesise_name_pool_from_generic(&mut free_names, &mut all_names, &mut used);
    }

    // ── 7. Name-pool expansion pass ────────────────────────────────────
    expand_free_names(&mut free_names, &all_names, &mut used, total_char_count);

    // ── 8. Character creation loop ─────────────────────────────────────
    world.log_debug("Creating random bot characters...");
    let mut total_random_bot_chars: u32 = 0;
    let mut bots_created: u32 = 0;

    // Shallow copy of the fixed-class/race config so we can mutate it
    // across account iterations. Matches the legacy C++ semantics where
    // counts are shared globally (once a (cls, race) counter reaches 0
    // it is erased for all subsequent accounts).
    let mut remaining: BTreeMap<(u8, u8), u32> = cfg.fixed_class_race_counts.clone();

    for account_number in 0..cfg.random_bot_account_count {
        let account_name = format!("{}{}", cfg.random_bot_account_prefix, account_number);
        let account_id = world.query_account_id_by_name(&account_name);
        if account_id == 0 {
            continue;
        }
        state.random_bot_accounts.push(account_id);

        let count = world.get_character_count(account_id);
        if count >= ACCOUNT_CHAR_LIMIT {
            total_random_bot_chars += count;
            continue;
        }

        if cfg.use_fixed_class_race_counts {
            let max_allowed = ACCOUNT_CHAR_LIMIT - count;
            let mut created: u32 = 0;

            while !remaining.is_empty() && created < max_allowed {
                // Shuffle the key set using the world RNG — matches the
                // legacy random_device + mt19937 shuffle.
                let mut keys: Vec<(u8, u8)> = remaining.keys().copied().collect();
                shuffle_with_world(world, &mut keys);

                for key in keys {
                    if created >= max_allowed {
                        break;
                    }

                    let (cls, race) = key;
                    if !is_available_race(cls, race) {
                        continue;
                    }

                    if create_random_bot(
                        cfg,
                        world,
                        account_id,
                        cls,
                        Some(race),
                        &mut free_names,
                    ) {
                        created += 1;
                        bots_created += 1;
                        if let Some(entry) = remaining.get_mut(&key) {
                            *entry -= 1;
                            if *entry == 0 {
                                remaining.remove(&key);
                            }
                        }
                    }
                }
            }
        } else {
            for cls in 1u8..(MAX_CLASSES as u8).saturating_sub(count as u8) {
                if !class_exists(cls) {
                    continue;
                }
                let random_cls = {
                    let mut rng = WorldRng { world };
                    get_random_class(cfg, &mut rng)
                };
                bots_created += 1;
                let _ = create_random_bot(
                    cfg,
                    world,
                    account_id,
                    random_cls,
                    None,
                    &mut free_names,
                );
            }
        }

        total_random_bot_chars += world.get_character_count(account_id);
    }

    if cfg.use_fixed_class_race_counts && !remaining.is_empty() {
        world.log_debug(
            "Unable to create all requested fixed class/race bots due to account character limits.",
        );
    }

    if bots_created == 0 {
        world.log_debug(&format!(
            "No new random bots needed. Accounts: {}, bots: {}.",
            state.random_bot_accounts.len(),
            total_random_bot_chars
        ));
        return;
    }

    // ── 9. Flush any bots created into the DB, log out all sessions ───
    world.save_and_logout_all_online();
    world.log_debug(&format!(
        "{} random bot accounts with {} characters available",
        state.random_bot_accounts.len(),
        total_random_bot_chars + bots_created
    ));
}

/// Port of the delete-pass inside `CreateRandomBots`.
///
/// Extracted into its own function because it is the largest chunk of
/// straight-line logic in the legacy method and benefits from being
/// isolated for readability.
fn delete_random_bots(
    cfg: &BotConfig,
    world: &dyn RandomFactoryWorld,
    del_friends: bool,
) {
    // Build the bot-account and friend-guid sets before scanning
    // characters so the inner loop can check both in O(1).
    let mut bot_accounts: Vec<u32> = Vec::with_capacity(cfg.random_bot_account_count as usize);
    for account_number in 0..cfg.random_bot_account_count {
        let account_name = format!("{}{}", cfg.random_bot_account_prefix, account_number);
        let id = world.query_account_id_by_name(&account_name);
        if id != 0 {
            bot_accounts.push(id);
        }
    }

    if del_friends {
        world.log_debug("Deleting all random bot characters...");
    } else {
        world.log_debug("Deleting random bot characters without friends/guild...");
    }

    let bot_friends: Vec<u32> = if del_friends {
        Vec::new()
    } else {
        world.query_friend_guids()
    };

    let matching_accounts =
        world.query_account_ids_like_prefix(&cfg.random_bot_account_prefix);

    for acc_id in matching_accounts {
        if del_friends {
            world.delete_account(acc_id);
            continue;
        }

        // Friends/guild-aware delete pass — preserve bots that are on
        // someone's friends list or in a guild whose leader is not a
        // random bot account.
        for guid_lo in world.query_characters_by_account(acc_id) {
            if bot_friends.contains(&guid_lo) {
                continue;
            }

            let guild_id = world.get_guild_id_by_leader(guid_lo);
            if guild_id != 0 {
                // The C++ check is actually the *player's* guild id,
                // resolved via `Player::GetGuildIdFromDB(guid)` — the
                // bridge collapses that and `sGuildMgr.GetGuildByLeader`
                // into a single `get_guild_id_by_leader` call that
                // returns the guild that this character leads (0 when
                // none). That matches the legacy filter's intent:
                // preserve bots that are in a guild led by a human.
                let account_id = world.get_guild_leader_account_id(guild_id);
                if !bot_accounts.contains(&account_id) {
                    continue;
                }
            }

            world.on_player_login_error(guid_lo);
            world.delete_character_from_db(guid_lo, acc_id);
        }
    }

    world.prune_random_bots_table();
    world.log_debug("Random bot characters deleted");
}

/// Build a 10-char random password matching the legacy loop
/// `for (i=0; i<10; i++) password += (char)urand('!', 'z')`.
fn generate_random_password(world: &dyn RandomFactoryWorld) -> String {
    let mut out = String::with_capacity(10);
    for _ in 0..10 {
        let c = world.urand_range(b'!' as u32, b'z' as u32) as u8 as char;
        out.push(c);
    }
    out
}

/// Split the name pool rows into free / all / used maps. The "used"
/// map mirrors the C++ `std::unordered_map<std::string, bool>` — the
/// bool value is ignored and the map is really a set membership check.
fn build_name_pools(
    rows: Vec<NamePoolRow>,
) -> (
    HashMap<NameRaceAndGender, Vec<String>>,
    HashMap<NameRaceAndGender, Vec<String>>,
    HashMap<String, bool>,
) {
    let mut free_names: HashMap<NameRaceAndGender, Vec<String>> = HashMap::new();
    let mut all_names: HashMap<NameRaceAndGender, Vec<String>> = HashMap::new();
    let mut used: HashMap<String, bool> = HashMap::new();

    for row in rows {
        let rag = NameRaceAndGender::from_raw(row.gender_index);
        let name = row.name;
        if !row.is_taken {
            free_names.entry(rag).or_default().push(name.clone());
        }
        all_names.entry(rag).or_default().push(name.clone());
        used.insert(name, false);
    }

    (free_names, all_names, used)
}

/// Fallback name synthesis used when `ai_playerbot_names` is not
/// loaded. Mirrors the "update the name database" branch in the
/// legacy C++ code which walks NameRaceAndGender variants 2..=17,
/// uppercases the first letter, and prefixes a bijective base-26
/// postfix onto existing generic names.
fn synthesise_name_pool_from_generic(
    free_names: &mut HashMap<NameRaceAndGender, Vec<String>>,
    all_names: &mut HashMap<NameRaceAndGender, Vec<String>>,
    used: &mut HashMap<String, bool>,
) {
    for type_idx in 2u8..=(NameRaceAndGender::BloodelfFemale.as_index() as u8) {
        let source_key = NameRaceAndGender::from_raw(type_idx % 2);
        let source_names = all_names.get(&source_key).cloned().unwrap_or_default();
        let target_key = NameRaceAndGender::from_raw(type_idx);

        for mut name in source_names {
            // Lowercase the first letter, prepend postfix, re-upper the
            // first letter. Matches the C++ `name[0] -= 'A' - 'a'` dance
            // that flips case before/after insertion.
            let mut chars: Vec<char> = name.chars().collect();
            if let Some(first) = chars.first_mut() {
                *first = first.to_ascii_lowercase();
            }
            name = chars.into_iter().collect();

            let post = name_postfix(i32::from(type_idx) - 2);
            let mut new_name = post;
            new_name.push_str(&name);

            let mut bytes = new_name.into_bytes();
            if let Some(first) = bytes.first_mut() {
                *first = first.to_ascii_uppercase();
            }
            let new_name = String::from_utf8(bytes).unwrap_or_default();

            all_names.entry(target_key).or_default().push(new_name.clone());
            if !used.contains_key(&new_name) {
                free_names
                    .entry(target_key)
                    .or_default()
                    .push(new_name.clone());
                used.insert(new_name, false);
            }
        }
    }
}

/// Name-pool expansion pass — ports the inner `namesNeeded` loop.
///
/// For every `NameRaceAndGender` variant whose free-pool is too small
/// to cover half the total-character-count target, grow the free pool
/// by concatenating ever-longer postfixes onto the variant's existing
/// `all_names` entries until the deficit is met or we exhaust the
/// 12-character name limit.
fn expand_free_names(
    free_names: &mut HashMap<NameRaceAndGender, Vec<String>>,
    all_names: &HashMap<NameRaceAndGender, Vec<String>>,
    used: &mut HashMap<String, bool>,
    total_char_count: u32,
) {
    let max_variant = NameRaceAndGender::BloodelfFemale.as_index() as u8;
    let half_target = (total_char_count / 2) as usize;

    for variant_idx in 0..=max_variant {
        let rag = NameRaceAndGender::from_raw(variant_idx);
        let free_len = free_names.get(&rag).map_or(0, Vec::len);
        if half_target < free_len {
            continue;
        }
        let mut names_needed = half_target - free_len;
        if names_needed == 0 {
            continue;
        }

        let mut new_names: Vec<String> = Vec::new();
        let mut post_itt: i32 = 0;

        let source = all_names.get(&rag).cloned().unwrap_or_default();
        'outer: while names_needed > 0 {
            let post = name_postfix(post_itt);
            for name in &source {
                if name.len() + post.len() > 12 {
                    continue;
                }
                let mut new_name = name.clone();
                new_name.push_str(&post);
                if used.contains_key(&new_name) {
                    continue;
                }
                used.insert(new_name.clone(), false);
                new_names.push(new_name);
                names_needed -= 1;
                if names_needed == 0 {
                    break 'outer;
                }
            }
            post_itt += 1;
            // Guard against runaway loops when the source list is empty
            // and we would otherwise spin forever on a zero-progress
            // iteration.
            if source.is_empty() {
                break;
            }
        }

        free_names.entry(rag).or_default().extend(new_names);
    }
}

/// Return true iff `cls` is a legitimate player class on the current
/// expansion. Mirrors the C++ `CLASSMASK_ALL_PLAYABLE` bit check plus
/// the expansion-specific class-6 (death knight) and class-10 guards.
fn class_exists(cls: u8) -> bool {
    if cls == 0 || cls >= MAX_CLASSES as u8 {
        return false;
    }
    if cls == 10 {
        return false;
    }
    #[cfg(not(feature = "wotlk"))]
    {
        if cls == 6 {
            return false;
        }
    }
    // Class 1..=11 excluding 10 (and 6 on non-wotlk) are all playable.
    true
}

/// In-place Fisher-Yates shuffle driven by the world's RNG. Used by
/// the fixed-class/race path to randomise the key order the way the
/// legacy C++ `std::shuffle(random_device, mt19937)` call did.
fn shuffle_with_world<T>(world: &dyn RandomFactoryWorld, slice: &mut [T]) {
    if slice.len() < 2 {
        return;
    }
    for i in (1..slice.len()).rev() {
        let j = world.urand_range(0, i as u32) as usize;
        slice.swap(i, j);
    }
}

/// Port of `RandomPlayerbotFactory::CreateRandomBot(cls, names, inputRace)`.
///
/// Picks a gender, resolves race (either `input_race` or a random roll),
/// pulls a free name out of the pool, resolves appearance from the
/// bridge's `query_char_appearance` callback, and finally asks the
/// bridge to create the character.
fn create_random_bot(
    cfg: &BotConfig,
    world: &dyn RandomFactoryWorld,
    account_id: u32,
    cls: u8,
    input_race: Option<u8>,
    free_names: &mut HashMap<NameRaceAndGender, Vec<String>>,
) -> bool {
    // `rand() % 2` in the C++; we use the world RNG for determinism.
    let gender = if world.urand_range(0, 1) != 0 {
        GENDER_MALE
    } else {
        GENDER_FEMALE
    };

    let race = match input_race {
        Some(r) => r,
        None => {
            let mut rng = WorldRng { world };
            get_random_race(cfg, cls, &mut rng)
        }
    };

    let race_and_gender = combine_race_and_gender(gender, race);

    // Pull a free name out of the pool (swap-remove to match the C++
    // `swap(names[rag][i], names[rag].back()); pop_back()`).
    let Some(pool) = free_names.get_mut(&race_and_gender) else {
        return false;
    };
    if pool.is_empty() {
        return false;
    }
    let idx = world.urand_range(0, (pool.len() - 1) as u32) as usize;
    let name = pool.swap_remove(idx);
    if name.is_empty() {
        return false;
    }

    // Resolve appearance. The bridge returns None when no sections are
    // available for this (race, gender) — we mirror the C++ "skip
    // silently and return false" fallthrough.
    let Some(appearance) = world.query_char_appearance(race, gender) else {
        return false;
    };

    // Tauren + most female races have no facial hair in the legacy
    // code. On WotLK the facial-hair field is always zero.
    #[cfg(not(feature = "wotlk"))]
    let facial_hair = {
        let exclude = race == RACE_TAUREN
            || (gender == GENDER_FEMALE && race != RACE_NIGHTELF && race != RACE_UNDEAD);
        if exclude { 0 } else { appearance.facial_hair }
    };
    #[cfg(feature = "wotlk")]
    let facial_hair = 0u8;

    let params = CreateParams {
        account_id,
        race,
        cls,
        gender,
        name,
        skin_color: appearance.face_color,
        face_id: appearance.face_id,
        face_color: appearance.face_color,
        hair_style: appearance.hair_style,
        hair_color: appearance.hair_color,
        facial_hair,
    };

    world.create_random_bot_character(&params)
}

/// Port of `RandomPlayerbotFactory::CreateRandomGuilds`.
///
/// Walks the character table, groups the guids by account, enumerates
/// the leaders that either already run a random-bot guild (tracked in
/// `state.random_bot_guilds`) or qualify as new guild leaders (level
/// ≥ 10, not already in a guild), and fills the pool up to
/// `cfg.random_bot_guild_count` with random emblems + names.
pub fn create_random_guilds(
    cfg: &BotConfig,
    world: &dyn RandomFactoryWorld,
    state: &mut FactoryState,
) {
    let char_accounts = world.query_character_accounts();
    if char_accounts.is_empty() {
        return;
    }

    // Group guids by account and keep only guids owned by one of our
    // random bot accounts.
    let mut char_acc_guids: HashMap<u32, Vec<u32>> = HashMap::new();
    for row in char_accounts {
        char_acc_guids
            .entry(row.account_id)
            .or_default()
            .push(row.guid_low);
    }

    let mut random_bots: Vec<u32> = Vec::new();
    for acc in &state.random_bot_accounts {
        if let Some(guids) = char_acc_guids.get(acc) {
            random_bots.extend(guids.iter().copied());
        }
    }

    if random_bots.is_empty() {
        return;
    }

    // Delete old guilds once per process.
    if cfg.delete_random_bot_guilds && !state.guilds_deleted {
        world.log_debug("Deleting random bot guilds...");
        let mut counter: u32 = 0;
        for leader in &random_bots {
            let guild_id = world.get_guild_id_by_leader(*leader);
            if guild_id != 0 {
                world.disband_guild(guild_id);
                counter += 1;
            }
        }
        world.log_debug(&format!("{counter} Random bot guilds deleted"));
        state.guilds_deleted = true;
    }

    if cfg.random_bot_guild_count == 0 {
        return;
    }

    // Scan guilds + candidate leaders.
    let mut guild_number: u32 = 0;
    let mut available_leaders: Vec<u32> = Vec::new();
    for leader in &random_bots {
        let guild_id = world.get_guild_id_by_leader(*leader);
        if guild_id != 0 {
            if !state.random_bot_guilds.contains(&guild_id) {
                guild_number += 1;
                state.random_bot_guilds.push(guild_id);
            }
        } else if let Some(snap) = world.get_player_snapshot(*leader) {
            if snap.guild_id == 0 && snap.level >= 10 {
                available_leaders.push(*leader);
            }
        }
    }

    if available_leaders.is_empty() {
        world.log_debug("No leaders for random guilds available");
        return;
    }

    // Guild creation loop.
    let mut attempts: u32 = 0;
    let max_new_guilds = cfg.random_bot_guild_count.saturating_sub(state.random_bot_guilds.len() as u32);
    let mut new_guilds_created = false;
    while guild_number < max_new_guilds {
        attempts += 1;
        if attempts > 5.min(cfg.random_bot_guild_count) {
            break;
        }
        if state.random_bot_guilds.len() as u32 >= cfg.random_bot_guild_count {
            break;
        }

        let Some(guild_name) = world.query_random_guild_name() else {
            continue;
        };
        if guild_name.is_empty() {
            continue;
        }

        let index = world.urand_range(0, (available_leaders.len() - 1) as u32) as usize;
        let leader = available_leaders[index];
        let Some(snap) = world.get_player_snapshot(leader) else {
            continue;
        };
        if snap.guild_id != 0 {
            continue;
        }

        let guild_id = world.create_random_guild(leader, &guild_name);
        if guild_id == 0 {
            continue;
        }

        let bg = world.urand_range(0, 51);
        let bc = world.urand_range(0, 17);
        let cl = world.urand_range(0, 17);
        let br = world.urand_range(0, 7);
        let st = world.urand_range(0, 180);
        world.set_guild_emblem(guild_id, st, cl, br, bc, bg);
        world.set_guild_info(guild_id, &world.urand_range(10, 30).to_string());

        state.random_bot_guilds.push(guild_id);
        world.log_debug(&format!(
            "Random Guild <{}>, GM: {}",
            guild_name, snap.name
        ));
        new_guilds_created = true;
        guild_number += 1;
    }

    if new_guilds_created {
        world.log_debug(&format!(
            "Total Random Guilds: {}",
            state.random_bot_guilds.len()
        ));
    }
}

/// Port of `RandomPlayerbotFactory::CreateRandomArenaTeams` (wotlk-only
/// in the legacy C++; compiles on TBC/WotLK builds in Rust but is
/// gated behind the expansion features so Classic builds don't pull
/// in the call graph at all).
#[cfg(any(feature = "tbc", feature = "wotlk"))]
pub fn create_random_arena_teams(
    cfg: &BotConfig,
    world: &dyn RandomFactoryWorld,
    state: &mut FactoryState,
) {
    const ARENA_TYPE_2V2: u8 = 2;
    const ARENA_TYPE_3V3: u8 = 3;
    const ARENA_TYPE_5V5: u8 = 5;

    let random_bots = world.query_random_bot_add_event();

    if cfg.delete_random_bot_arena_teams && !state.arena_teams_deleted {
        world.log_debug("Deleting random bot arena teams...");
        for captain in &random_bots {
            if let Some((team_id, _ty)) = world.get_arena_team_by_captain(*captain) {
                world.disband_arena_team(team_id);
            }
        }
        world.log_debug("Random bot arena teams deleted");
        state.arena_teams_deleted = true;
    }

    let mut teams_number: HashMap<u8, u32> = HashMap::new();
    let mut max_teams_number: HashMap<u8, u32> = HashMap::new();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        max_teams_number.insert(
            ARENA_TYPE_2V2,
            (f64::from(cfg.random_bot_arena_team_count) * 0.4) as u32,
        );
        max_teams_number.insert(
            ARENA_TYPE_3V3,
            (f64::from(cfg.random_bot_arena_team_count) * 0.3) as u32,
        );
        max_teams_number.insert(
            ARENA_TYPE_5V5,
            (f64::from(cfg.random_bot_arena_team_count) * 0.3) as u32,
        );
    }

    let mut available_captains: Vec<u32> = Vec::new();
    for captain in &random_bots {
        if let Some((team_id, ty)) = world.get_arena_team_by_captain(*captain) {
            *teams_number.entry(ty).or_insert(0) += 1;
            state.random_bot_arena_teams.push(team_id);
        }

        let Some(snap) = world.get_player_snapshot(*captain) else {
            continue;
        };
        if snap.level < DEFAULT_MAX_LEVEL {
            continue;
        }

        if world.get_player_arena_team_id_by_type(*captain, ARENA_TYPE_2V2) != 0 {
            continue;
        }
        if world.get_player_arena_team_id_by_type(*captain, ARENA_TYPE_3V3) != 0 {
            continue;
        }
        if world.get_player_arena_team_id_by_type(*captain, ARENA_TYPE_5V5) != 0 {
            continue;
        }

        available_captains.push(*captain);
    }

    let mut attempts: u32 = 0;
    let mut arena_team_number: u32 = 0;
    while arena_team_number < cfg.random_bot_arena_team_count {
        if attempts > cfg.random_bot_arena_team_count {
            break;
        }

        let random_type = match world.urand_range(0, 2) {
            0 => ARENA_TYPE_2V2,
            1 => ARENA_TYPE_3V3,
            _ => ARENA_TYPE_5V5,
        };

        let Some((arena_team_name, slot)) = world.query_random_arena_team_name() else {
            continue;
        };
        if arena_team_name.is_empty() {
            continue;
        }

        if available_captains.is_empty() {
            world.log_debug("No captains for random arena teams available");
            continue;
        }

        let index = world.urand_range(0, (available_captains.len() - 1) as u32) as usize;
        let captain = available_captains[index];
        let Some(snap) = world.get_player_snapshot(captain) else {
            continue;
        };
        if snap.level < DEFAULT_MAX_LEVEL {
            world.log_debug(&format!(
                "Bot {} must be level {} to create an arena team",
                captain, DEFAULT_MAX_LEVEL
            ));
            continue;
        }

        let ty = match slot {
            2 => ARENA_TYPE_2V2,
            3 => ARENA_TYPE_3V3,
            5 => ARENA_TYPE_5V5,
            _ => ARENA_TYPE_2V2,
        };

        attempts += 1;

        if ty != random_type {
            continue;
        }
        if teams_number.get(&ty).copied().unwrap_or(0)
            >= max_teams_number.get(&ty).copied().unwrap_or(0)
        {
            continue;
        }
        if world.get_player_arena_team_id_by_type(captain, ty) != 0 {
            continue;
        }

        let team_id = world.create_arena_team(captain, ty, &arena_team_name);
        if team_id == 0 {
            continue;
        }

        let background_color = world.urand_range(0xFF00_0000, 0xFFFF_FFFF);
        let emblem_style = world.urand_range(0, 101);
        let emblem_color = world.urand_range(0xFF00_0000, 0xFFFF_FFFF);
        let border_style = world.urand_range(0, 5);
        let border_color = world.urand_range(0xFF00_0000, 0xFFFF_FFFF);
        world.set_arena_team_emblem(
            team_id,
            background_color,
            emblem_style,
            emblem_color,
            border_style,
            border_color,
        );
        state.random_bot_arena_teams.push(team_id);

        for _ in 0..10 {
            if world.get_arena_team_members_size(team_id) >= u32::from(ty) {
                break;
            }
            let idx = world.urand_range(0, (available_captains.len() - 1) as u32) as usize;
            let possible_member = available_captains[idx];
            if possible_member == captain {
                continue;
            }
            let Some(member) = world.get_player_snapshot(possible_member) else {
                continue;
            };
            if member.team != snap.team {
                continue;
            }
            if !world.add_arena_team_member(team_id, possible_member) {
                continue;
            }
        }

        if world.get_arena_team_members_size(team_id) < u32::from(ty) {
            world.log_debug(&format!(
                "Random Arena team {arena_team_name}: failed to get enough members, deleting..."
            ));
            world.disband_arena_team(team_id);
            return;
        }

        world.set_arena_team_rating(team_id, world.urand_range(1500, 2700));
        world.save_arena_team(team_id);
        world.log_debug(&format!("Random Arena team {arena_team_name}: created"));
        arena_team_number += 1;
    }

    world.log_debug(&format!(
        "{arena_team_number} random bot arena teams available"
    ));
}

/// Stub implementation on Classic — the legacy C++ does not compile
/// the arena-team path at all on `MANGOSBOT_ZERO`, so the Rust stub
/// simply returns without touching the state.
#[cfg(not(any(feature = "tbc", feature = "wotlk")))]
pub fn create_random_arena_teams(
    _cfg: &BotConfig,
    _world: &dyn RandomFactoryWorld,
    _state: &mut FactoryState,
) {
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::{BotDeleteEvent, CharacterAccount, MockRandomFactoryWorld, PlayerSnapshot};

    fn make_cfg() -> BotConfig {
        BotConfig::default()
    }

    #[test]
    fn create_random_bots_auto_create_disabled_records_existing_accounts() {
        let world = MockRandomFactoryWorld::new();
        let mut cfg = make_cfg();
        cfg.random_bot_auto_create = false;
        cfg.random_bot_account_prefix = "rndbot".to_string();
        cfg.random_bot_account_count = 3;

        world.set_account_id_by_name("rndbot0", 100);
        world.set_account_id_by_name("rndbot1", 101);
        // rndbot2 missing on purpose

        let mut state = FactoryState::new();
        create_random_bots(&cfg, &world, &mut state);

        assert_eq!(state.random_bot_accounts, vec![100, 101]);
    }

    #[test]
    fn create_random_bots_delete_pass_runs_when_scheduled() {
        let world = MockRandomFactoryWorld::new();
        let mut cfg = make_cfg();
        cfg.random_bot_account_prefix = "rndbot".to_string();
        cfg.random_bot_account_count = 1;
        cfg.random_bot_auto_create = false;

        world.set_bot_delete_event(BotDeleteEvent {
            scheduled: true,
            delete_friends: true,
        });
        world.set_accounts_by_prefix("rndbot", vec![100, 101]);

        let mut state = FactoryState::new();
        create_random_bots(&cfg, &world, &mut state);

        // Delete-friends path should call delete_account for every
        // matching account.
        let events = world.events();
        let deleted: Vec<u32> = events
            .iter()
            .filter_map(|e| match e {
                cmangos::MockRandomFactoryEvent::DeleteAccount(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(deleted, vec![100, 101]);
    }

    #[test]
    fn create_random_guilds_skips_when_state_has_no_bots() {
        let world = MockRandomFactoryWorld::new();
        world.set_character_accounts(vec![CharacterAccount {
            account_id: 100,
            guid_low: 1,
        }]);
        let cfg = make_cfg();
        let mut state = FactoryState::new();
        create_random_guilds(&cfg, &world, &mut state);
        assert!(state.random_bot_guilds.is_empty());
    }

    #[test]
    fn create_random_guilds_picks_eligible_leaders() {
        let world = MockRandomFactoryWorld::new();
        let mut cfg = make_cfg();
        cfg.random_bot_guild_count = 1;
        // Two random bot accounts each with one character.
        world.set_character_accounts(vec![
            CharacterAccount {
                account_id: 100,
                guid_low: 500,
            },
            CharacterAccount {
                account_id: 101,
                guid_low: 501,
            },
        ]);
        world.set_player_snapshot(
            500,
            PlayerSnapshot {
                level: 15,
                guild_id: 0,
                team: 0,
                name: "Arthas".to_string(),
            },
        );
        world.set_player_snapshot(
            501,
            PlayerSnapshot {
                level: 5, // too low — should be ignored
                guild_id: 0,
                team: 0,
                name: "Anduin".to_string(),
            },
        );
        world.push_random_guild_name("Stormwind Bots".to_string());
        world.set_create_guild_result(500, 42);

        let mut state = FactoryState::new();
        state.random_bot_accounts = vec![100, 101];
        create_random_guilds(&cfg, &world, &mut state);

        assert_eq!(state.random_bot_guilds, vec![42]);
    }

    #[test]
    fn class_exists_rejects_cls_10() {
        assert!(!class_exists(10));
    }

    #[test]
    fn shuffle_with_world_is_stable_for_single_element() {
        let world = MockRandomFactoryWorld::new();
        let mut v = vec![42u32];
        shuffle_with_world(&world, &mut v);
        assert_eq!(v, vec![42]);
    }

    #[test]
    fn build_name_pools_splits_taken_rows() {
        let rows = vec![
            NamePoolRow {
                gender_index: 4, // DwarfMale
                name: "Bromir".to_string(),
                is_taken: false,
            },
            NamePoolRow {
                gender_index: 4,
                name: "Thrall".to_string(),
                is_taken: true,
            },
        ];
        let (free, all, used) = build_name_pools(rows);
        assert_eq!(free.get(&NameRaceAndGender::DwarfMale).unwrap().len(), 1);
        assert_eq!(all.get(&NameRaceAndGender::DwarfMale).unwrap().len(), 2);
        assert!(used.contains_key("Bromir"));
        assert!(used.contains_key("Thrall"));
    }

    #[test]
    fn expand_free_names_grows_short_pool_via_postfix() {
        let mut free: HashMap<NameRaceAndGender, Vec<String>> = HashMap::new();
        free.insert(NameRaceAndGender::GenericMale, vec!["Bob".to_string()]);
        let mut all: HashMap<NameRaceAndGender, Vec<String>> = HashMap::new();
        all.insert(
            NameRaceAndGender::GenericMale,
            vec!["Bob".to_string(), "Max".to_string()],
        );
        let mut used: HashMap<String, bool> = HashMap::new();
        used.insert("Bob".to_string(), false);
        used.insert("Max".to_string(), false);

        // Target 4 → half = 2 → needs 1 more beyond existing 1.
        expand_free_names(&mut free, &all, &mut used, 4);

        let generic = free.get(&NameRaceAndGender::GenericMale).unwrap();
        assert!(generic.len() >= 2);
    }

    #[test]
    fn delete_default_max_level_gate_works() {
        // Smoke test DEFAULT_MAX_LEVEL matches the active feature gate.
        #[cfg(feature = "wotlk")]
        assert_eq!(DEFAULT_MAX_LEVEL, 80);
        #[cfg(all(feature = "tbc", not(feature = "wotlk")))]
        assert_eq!(DEFAULT_MAX_LEVEL, 70);
        #[cfg(not(any(feature = "tbc", feature = "wotlk")))]
        assert_eq!(DEFAULT_MAX_LEVEL, 60);
    }
}
