//! `RandomFactoryWorld` — module-level interface the Rust random bot
//! factory uses to talk to `CMaNGOS`.
//!
//! Phase G of the C++ → Rust migration ports
//! `playerbot/RandomPlayerbotFactory.cpp`. Like [`LoginWorld`](crate::LoginWorld)
//! and [`ItemWorld`](crate::ItemWorld), the interface is shared — none of
//! the calls are per-bot. It covers every CMaNGOS-side operation the
//! legacy factory performed: DB scans over accounts, characters, the name
//! pool, friends, guilds, and arena teams; account creation and deletion;
//! `Player::Create` + object accessor bookkeeping; guild/arena team
//! lifecycle helpers on `sGuildMgr`/`sObjectMgr`.
//!
//! Two implementations exist, mirroring `LoginWorld`:
//!
//! * [`VtableRandomFactoryWorld`] — production impl, wraps a
//!   `RandomFactoryCallbacks` struct handed in by the C++ bridge.
//! * [`MockRandomFactoryWorld`] — feature-gated under `mock`; a pure-Rust
//!   fixture impl used by the `crates/playerbot` random factory tests.

use crate::{
    BotCharAppearance, BotCharacterAccount, BotCreateParams, BotDeleteEventState, BotNamePoolRow,
    BotPlayerSnapshot, RandomFactoryCallbacks,
};

/// Rust-side decoding of `BotDeleteEventState`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct BotDeleteEvent {
    /// True iff an `ai_playerbot_random_bots.event = 'bot_delete'` row
    /// exists — the signal that the random bot pool must be wiped.
    pub scheduled: bool,
    /// When the DB `value` column is > 1: true means "delete every bot
    /// character regardless of friend / guild membership". When false,
    /// friends and guild members are preserved (the legacy C++ default).
    pub delete_friends: bool,
}

/// Rust-side decoding of `BotNamePoolRow`.
///
/// `gender_index` matches the `NameRaceAndGender` values in the
/// `crates/playerbot` random module; `is_taken` is true iff the row was
/// produced by joining `ai_playerbot_names` against `characters` and a
/// character with this name already exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamePoolRow {
    pub gender_index: u8,
    pub name: String,
    pub is_taken: bool,
}

/// Rust-side decoding of `BotCharAppearance`. The `facial_hair` field is
/// zero for races / genders without facial hair (tauren, non-night-elf
/// non-undead females) and always zero on `WotLK` — the C++ `excludeCheck`
/// logic lives on the bridge side, not in Rust.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct CharAppearance {
    pub skin_color: u8,
    pub face_id: u8,
    pub face_color: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
}

/// Parameter bundle for `create_random_bot_character`. Mirrors the local
/// variables the legacy C++ `CreateRandomBot` assembled before calling
/// `Player::Create`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateParams {
    pub account_id: u32,
    pub race: u8,
    pub cls: u8,
    pub gender: u8,
    pub name: String,
    pub skin_color: u8,
    pub face_id: u8,
    pub face_color: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
}

/// Rust-side decoding of `BotPlayerSnapshot`. Only filled when the guid
/// points at a currently-logged-in player.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerSnapshot {
    pub level: u8,
    pub guild_id: u32,
    pub team: u32,
    pub name: String,
}

/// Rust-side decoding of `BotCharacterAccount`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct CharacterAccount {
    pub account_id: u32,
    pub guid_low: u32,
}

/// Abstract interface the random bot factory uses to reach `CMaNGOS`.
///
/// Every method is invoked from the thread that runs the
/// `create_random_bots` / `create_random_guilds` /
/// `create_random_arena_teams` orchestration functions. The factory runs
/// synchronously — there is no worker thread, unlike
/// [`LoginWorld`](crate::LoginWorld).
///
/// Default implementations that return empty / `None` / `0` mirror the
/// "missing callback" behaviour of [`VtableRandomFactoryWorld`]: on
/// Classic and TBC the arena team slots are expected to be NULL in the
/// C vtable, so calling them from an orchestration path that has been
/// gated on the expansion feature is a programmer error rather than a
/// runtime concern.
pub trait RandomFactoryWorld: Send + Sync {
    // ─── RNG ──────────────────────────────────────────────────────────
    /// Pseudo-random integer in `[min, max]` (both inclusive). Wraps
    /// `CMaNGOS` `urand`.
    fn urand_range(&self, min: u32, max: u32) -> u32;

    // ─── Bot delete event ─────────────────────────────────────────────
    /// Read `ai_playerbot_random_bots WHERE event = 'bot_delete'`.
    /// Returns the default zero value when no row is present.
    fn query_bot_delete_event(&self) -> BotDeleteEvent;

    // ─── Account scan / create / delete ───────────────────────────────
    /// Look up an account id by exact username match. Returns 0 on miss.
    fn query_account_id_by_name(&self, name: &str) -> u32;

    /// Enumerate every account id whose username begins with `prefix`
    /// (case-insensitive).
    fn query_account_ids_like_prefix(&self, prefix: &str) -> Vec<u32>;

    /// Create an account. Empty `password` means "use the account name
    /// as the password". `max_expansion` is `MAX_EXPANSION` on TBC/WotLK
    /// and 0 on Classic.
    fn create_account(&self, name: &str, password: &str, max_expansion: u8);

    /// Delete an account and every character on it.
    fn delete_account(&self, account_id: u32);

    // ─── Character count / query / friend list ────────────────────────
    /// `sAccountMgr.GetCharactersCount(account_id)`.
    fn get_character_count(&self, account_id: u32) -> u32;

    /// Enumerate every character guid on `account_id`.
    fn query_characters_by_account(&self, account_id: u32) -> Vec<u32>;

    /// Read every friend guid from `character_social` where
    /// `flags = SOCIAL_FLAG_FRIEND`.
    fn query_friend_guids(&self) -> Vec<u32>;

    /// `sRandomPlayerbotMgr.OnPlayerLoginError(guid)` — invoked before a
    /// character row is deleted so the login queue drops any pending
    /// state for it.
    fn on_player_login_error(&self, guid_low: u32);

    /// `Player::DeleteFromDB(guid, account, false, true)`.
    fn delete_character_from_db(&self, guid_low: u32, account_id: u32);

    /// `CharacterDatabase.Execute("DELETE FROM ai_playerbot_random_bots
    /// WHERE bot NOT IN (SELECT guid FROM characters)")`.
    fn prune_random_bots_table(&self);

    // ─── Name pool ────────────────────────────────────────────────────
    /// Load every row of `ai_playerbot_names` joined against `characters`.
    fn query_name_pool(&self) -> Vec<NamePoolRow>;

    /// Fallback load used when `ai_playerbot_names` is unpopulated —
    /// returns every existing character name as a `NamePoolRow` with
    /// `gender_index = 0` and `is_taken = true`.
    fn query_existing_character_names(&self) -> Vec<NamePoolRow>;

    /// Single-row fallback used by `CreateRandomBotName` when the Rust
    /// name pool is empty for a given race/gender. Returns `None` when
    /// no row is available.
    fn query_random_bot_name(&self, race_and_gender: u8) -> Option<String>;

    // ─── Character creation ───────────────────────────────────────────
    /// Resolve an appearance slice from `sCharSectionMap` for `(race,
    /// gender)`. `None` signals "no matching sections exist", i.e. the
    /// orchestrator should skip this class/race pair.
    fn query_char_appearance(&self, race: u8, gender: u8) -> Option<CharAppearance>;

    /// Create a single random-bot character. Wraps `WorldSession`
    /// allocation, `Player::Create`, `SetAtLoginFlag`, and
    /// `sObjectAccessor.AddObject`. Returns `true` on success.
    fn create_random_bot_character(&self, params: &CreateParams) -> bool;

    /// Save every currently-in-world random bot player and tear them
    /// down (`SaveToDB` + `LogoutPlayer` + delete-session delete-player
    /// loop). Called once at the end of `CreateRandomBots`.
    fn save_and_logout_all_online(&self);

    // ─── Guild enumeration / creation ─────────────────────────────────
    /// Scan every `(account, guid)` pair from the `characters` table.
    fn query_character_accounts(&self) -> Vec<CharacterAccount>;

    /// `sGuildMgr.GetGuildByLeader(guid)->GetId()`, 0 on miss.
    fn get_guild_id_by_leader(&self, leader_guid_low: u32) -> u32;

    /// `sObjectMgr.GetPlayerAccountIdByGUID(guild->GetLeaderGuid())`.
    fn get_guild_leader_account_id(&self, guild_id: u32) -> u32;

    /// `guild->Disband()`.
    fn disband_guild(&self, guild_id: u32);

    /// `sObjectMgr.GetPlayer(guid)` wrapper — returns `None` when the
    /// guid is not a currently logged-in player.
    fn get_player_snapshot(&self, guid_low: u32) -> Option<PlayerSnapshot>;

    /// Single-row query against `ai_playerbot_guild_names`.
    fn query_random_guild_name(&self) -> Option<String>;

    /// `Guild::Create(player, name) + sGuildMgr.AddGuild` — returns the
    /// new guild id, or 0 on failure.
    fn create_random_guild(&self, leader_guid_low: u32, name: &str) -> u32;

    /// `guild->SetEmblem(st, cl, br, bc, bg)`.
    fn set_guild_emblem(
        &self,
        guild_id: u32,
        style: u32,
        color: u32,
        border_style: u32,
        border_color: u32,
        background_color: u32,
    );

    /// `guild->SetGINFO(info)`.
    fn set_guild_info(&self, guild_id: u32, info: &str);

    // ─── Arena team (wotlk only on the C side) ────────────────────────
    /// Read `ai_playerbot_random_bots WHERE event = 'add'` — the captain
    /// candidate pool used by `CreateRandomArenaTeams`.
    fn query_random_bot_add_event(&self) -> Vec<u32> {
        Vec::new()
    }

    /// `sObjectMgr.GetArenaTeamByCaptain(guid)` — returns
    /// `Some((team_id, team_type))` when a team exists.
    fn get_arena_team_by_captain(&self, _captain_guid_low: u32) -> Option<(u32, u8)> {
        None
    }

    /// `arenateam->Disband(NULL)`.
    fn disband_arena_team(&self, _team_id: u32) {}

    /// `player->GetArenaTeamId(ArenaTeam::GetSlotByType(team_type))`.
    fn get_player_arena_team_id_by_type(&self, _guid_low: u32, _team_type: u8) -> u32 {
        0
    }

    /// Single-row query against `ai_playerbot_arena_team_names` —
    /// returns `(name, type)` where `type` is one of the
    /// `PLAYERBOT_ARENA_TYPE_*` values.
    fn query_random_arena_team_name(&self) -> Option<(String, u8)> {
        None
    }

    /// `ArenaTeam::Create + SetCaptain + sObjectMgr.AddArenaTeam` —
    /// returns the new team id, or 0 on failure.
    fn create_arena_team(&self, _captain_guid_low: u32, _team_type: u8, _name: &str) -> u32 {
        0
    }

    /// `arenateam->AddMember(member_guid)`.
    fn add_arena_team_member(&self, _team_id: u32, _member_guid_low: u32) -> bool {
        false
    }

    /// `arenateam->GetMembersSize()`.
    fn get_arena_team_members_size(&self, _team_id: u32) -> u32 {
        0
    }

    /// `arenateam->SetEmblem(background, style, color, border_style, border_color)`.
    fn set_arena_team_emblem(
        &self,
        _team_id: u32,
        _background_color: u32,
        _style: u32,
        _color: u32,
        _border_style: u32,
        _border_color: u32,
    ) {
    }

    /// `arenateam->SetRatingForAll(rating)`.
    fn set_arena_team_rating(&self, _team_id: u32, _rating: u32) {}

    /// `arenateam->SaveToDB()`.
    fn save_arena_team(&self, _team_id: u32) {}

    // ─── Debug ────────────────────────────────────────────────────────
    /// Forward a debug line to the C++ log sink. Default: no-op.
    fn log_debug(&self, _msg: &str) {}
}

/// Production impl — wraps the C `RandomFactoryCallbacks` vtable.
///
/// `VtableRandomFactoryWorld` is `Send + Sync` because every stored
/// function pointer targets global C++ state protected by
/// `sObjectAccessor` / `CharacterDatabase` / `sGuildMgr` internal locks
/// and the factory is only invoked from a single orchestration thread
/// during startup / random-bot refill passes.
pub struct VtableRandomFactoryWorld {
    cbs: RandomFactoryCallbacks,
}

#[allow(unsafe_code)]
// Safety: the stored function pointers target thread-safe C++ state per
// the invariants above; `RandomFactoryCallbacks` is `Copy` and contains
// no owned resources.
unsafe impl Send for VtableRandomFactoryWorld {}
#[allow(unsafe_code)]
unsafe impl Sync for VtableRandomFactoryWorld {}

impl VtableRandomFactoryWorld {
    /// # Safety
    /// `cbs` must be a fully initialised `RandomFactoryCallbacks` whose
    /// function pointers remain valid for the entire lifetime of the
    /// returned object (in practice: until server shutdown).
    #[must_use]
    #[allow(unsafe_code)]
    pub unsafe fn new(cbs: RandomFactoryCallbacks) -> Self {
        Self { cbs }
    }
}

/// Copy a Rust `&str` into a stack-local NUL-terminated buffer of
/// capacity `N`. Returns the buffer; callers pass `buf.as_ptr().cast()`
/// to C. Truncation silently happens when the input is longer than
/// `N - 1` bytes.
fn make_c_buf<const N: usize>(s: &str) -> [u8; N] {
    let mut buf = [0u8; N];
    let bytes = s.as_bytes();
    let n = bytes.len().min(N - 1);
    buf[..n].copy_from_slice(&bytes[..n]);
    buf[n] = 0;
    buf
}

/// Decode a NUL-terminated `char[N]` field (from a C POD struct) as a
/// lossy Rust `String`. Stops at the first NUL byte; non-UTF-8 bytes
/// become `U+FFFD`.
fn c_field_to_string(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

#[allow(unsafe_code)]
impl RandomFactoryWorld for VtableRandomFactoryWorld {
    fn urand_range(&self, min: u32, max: u32) -> u32 {
        self.cbs
            .urand_range
            .map_or(min, |f| unsafe { f(min, max) })
    }

    fn query_bot_delete_event(&self) -> BotDeleteEvent {
        let Some(f) = self.cbs.query_bot_delete_event else {
            return BotDeleteEvent::default();
        };
        let mut out = BotDeleteEventState {
            scheduled: false,
            delete_friends: false,
        };
        // Safety: out is a valid BotDeleteEventState on the stack.
        unsafe { f(&raw mut out) };
        BotDeleteEvent {
            scheduled: out.scheduled,
            delete_friends: out.delete_friends,
        }
    }

    fn query_account_id_by_name(&self, name: &str) -> u32 {
        let Some(f) = self.cbs.query_account_id_by_name else {
            return 0;
        };
        let buf = make_c_buf::<64>(name);
        unsafe { f(buf.as_ptr().cast()) }
    }

    fn query_account_ids_like_prefix(&self, prefix: &str) -> Vec<u32> {
        let Some(query) = self.cbs.query_account_ids_like_prefix else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_u32_list else {
            return Vec::new();
        };

        let buf = make_c_buf::<64>(prefix);
        let mut ptr: *mut u32 = core::ptr::null_mut();
        // Safety: C side allocates into *out_ids, we free via paired callback.
        let count = unsafe { query(buf.as_ptr().cast(), &raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        // Safety: C guarantees `count` readable u32s.
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out = slice.to_vec();
        // Safety: paired free.
        unsafe { free(ptr, count) };
        out
    }

    fn create_account(&self, name: &str, password: &str, max_expansion: u8) {
        let Some(f) = self.cbs.create_account else {
            return;
        };
        let name_buf = make_c_buf::<64>(name);
        let pass_buf = make_c_buf::<64>(password);
        unsafe {
            f(
                name_buf.as_ptr().cast(),
                pass_buf.as_ptr().cast(),
                max_expansion,
            );
        };
    }

    fn delete_account(&self, account_id: u32) {
        if let Some(f) = self.cbs.delete_account {
            unsafe { f(account_id) };
        }
    }

    fn get_character_count(&self, account_id: u32) -> u32 {
        self.cbs
            .get_character_count
            .map_or(0, |f| unsafe { f(account_id) })
    }

    fn query_characters_by_account(&self, account_id: u32) -> Vec<u32> {
        let Some(query) = self.cbs.query_characters_by_account else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_u32_list else {
            return Vec::new();
        };
        let mut ptr: *mut u32 = core::ptr::null_mut();
        let count = unsafe { query(account_id, &raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out = slice.to_vec();
        unsafe { free(ptr, count) };
        out
    }

    fn query_friend_guids(&self) -> Vec<u32> {
        let Some(query) = self.cbs.query_friend_guids else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_u32_list else {
            return Vec::new();
        };
        let mut ptr: *mut u32 = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out = slice.to_vec();
        unsafe { free(ptr, count) };
        out
    }

    fn on_player_login_error(&self, guid_low: u32) {
        if let Some(f) = self.cbs.on_player_login_error {
            unsafe { f(guid_low) };
        }
    }

    fn delete_character_from_db(&self, guid_low: u32, account_id: u32) {
        if let Some(f) = self.cbs.delete_character_from_db {
            unsafe { f(guid_low, account_id) };
        }
    }

    fn prune_random_bots_table(&self) {
        if let Some(f) = self.cbs.prune_random_bots_table {
            unsafe { f() };
        }
    }

    fn query_name_pool(&self) -> Vec<NamePoolRow> {
        query_name_pool_inner(self.cbs.query_name_pool, self.cbs.free_name_pool_list)
    }

    fn query_existing_character_names(&self) -> Vec<NamePoolRow> {
        query_name_pool_inner(
            self.cbs.query_existing_character_names,
            self.cbs.free_name_pool_list,
        )
    }

    fn query_random_bot_name(&self, race_and_gender: u8) -> Option<String> {
        let f = self.cbs.query_random_bot_name?;
        let mut buf = [0u8; 16];
        let ok = unsafe { f(race_and_gender, buf.as_mut_ptr().cast(), buf.len() as u32) };
        if !ok {
            return None;
        }
        let s = c_field_to_string(&buf);
        if s.is_empty() { None } else { Some(s) }
    }

    fn query_char_appearance(&self, race: u8, gender: u8) -> Option<CharAppearance> {
        let f = self.cbs.query_char_appearance?;
        let mut out = BotCharAppearance {
            skin_color: 0,
            face_id: 0,
            face_color: 0,
            hair_style: 0,
            hair_color: 0,
            facial_hair: 0,
            success: false,
        };
        let ok = unsafe { f(race, gender, &raw mut out) };
        if !ok || !out.success {
            return None;
        }
        Some(CharAppearance {
            skin_color: out.skin_color,
            face_id: out.face_id,
            face_color: out.face_color,
            hair_style: out.hair_style,
            hair_color: out.hair_color,
            facial_hair: out.facial_hair,
        })
    }

    fn create_random_bot_character(&self, params: &CreateParams) -> bool {
        let Some(f) = self.cbs.create_random_bot_character else {
            return false;
        };
        let name_buf = make_c_buf::<16>(&params.name);
        let mut ffi = BotCreateParams {
            account_id: params.account_id,
            race: params.race,
            cls: params.cls,
            gender: params.gender,
            name: [0; 16],
            skin_color: params.skin_color,
            face_id: params.face_id,
            face_color: params.face_color,
            hair_style: params.hair_style,
            hair_color: params.hair_color,
            facial_hair: params.facial_hair,
        };
        // Copy the 16-byte name buffer across (the C field is
        // `char name[16]`, so bindgen gives us `[c_char; 16]`).
        for (dst, src) in ffi.name.iter_mut().zip(name_buf.iter()) {
            *dst = *src as _;
        }
        unsafe { f(&raw const ffi) }
    }

    fn save_and_logout_all_online(&self) {
        if let Some(f) = self.cbs.save_and_logout_all_online {
            unsafe { f() };
        }
    }

    fn query_character_accounts(&self) -> Vec<CharacterAccount> {
        let Some(query) = self.cbs.query_character_accounts else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_character_account_list else {
            return Vec::new();
        };
        let mut ptr: *mut BotCharacterAccount = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out: Vec<CharacterAccount> = slice
            .iter()
            .map(|r| CharacterAccount {
                account_id: r.account_id,
                guid_low: r.guid_low,
            })
            .collect();
        unsafe { free(ptr, count) };
        out
    }

    fn get_guild_id_by_leader(&self, leader_guid_low: u32) -> u32 {
        self.cbs
            .get_guild_id_by_leader
            .map_or(0, |f| unsafe { f(leader_guid_low) })
    }

    fn get_guild_leader_account_id(&self, guild_id: u32) -> u32 {
        self.cbs
            .get_guild_leader_account_id
            .map_or(0, |f| unsafe { f(guild_id) })
    }

    fn disband_guild(&self, guild_id: u32) {
        if let Some(f) = self.cbs.disband_guild {
            unsafe { f(guild_id) };
        }
    }

    fn get_player_snapshot(&self, guid_low: u32) -> Option<PlayerSnapshot> {
        let f = self.cbs.get_player_snapshot?;
        let mut out = BotPlayerSnapshot {
            in_world: false,
            level: 0,
            guild_id: 0,
            team: 0,
            name: [0; 16],
        };
        let ok = unsafe { f(guid_low, &raw mut out) };
        if !ok || !out.in_world {
            return None;
        }
        // SAFETY: bindgen produces `[c_char; 16]` which is a POD byte
        // array that we can reinterpret as `u8` for string decoding.
        let name_bytes: [u8; 16] = out.name.map(|b| b as u8);
        Some(PlayerSnapshot {
            level: out.level,
            guild_id: out.guild_id,
            team: out.team,
            name: c_field_to_string(&name_bytes),
        })
    }

    fn query_random_guild_name(&self) -> Option<String> {
        let f = self.cbs.query_random_guild_name?;
        let mut buf = [0u8; 32];
        let ok = unsafe { f(buf.as_mut_ptr().cast(), buf.len() as u32) };
        if !ok {
            return None;
        }
        let s = c_field_to_string(&buf);
        if s.is_empty() { None } else { Some(s) }
    }

    fn create_random_guild(&self, leader_guid_low: u32, name: &str) -> u32 {
        let Some(f) = self.cbs.create_random_guild else {
            return 0;
        };
        let buf = make_c_buf::<32>(name);
        unsafe { f(leader_guid_low, buf.as_ptr().cast()) }
    }

    fn set_guild_emblem(
        &self,
        guild_id: u32,
        style: u32,
        color: u32,
        border_style: u32,
        border_color: u32,
        background_color: u32,
    ) {
        if let Some(f) = self.cbs.set_guild_emblem {
            unsafe {
                f(
                    guild_id,
                    style,
                    color,
                    border_style,
                    border_color,
                    background_color,
                );
            };
        }
    }

    fn set_guild_info(&self, guild_id: u32, info: &str) {
        let Some(f) = self.cbs.set_guild_info else {
            return;
        };
        let buf = make_c_buf::<256>(info);
        unsafe { f(guild_id, buf.as_ptr().cast()) };
    }

    fn query_random_bot_add_event(&self) -> Vec<u32> {
        let Some(query) = self.cbs.query_random_bot_add_event else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_u32_list else {
            return Vec::new();
        };
        let mut ptr: *mut u32 = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out = slice.to_vec();
        unsafe { free(ptr, count) };
        out
    }

    fn get_arena_team_by_captain(&self, captain_guid_low: u32) -> Option<(u32, u8)> {
        let f = self.cbs.get_arena_team_by_captain?;
        let mut team_id: u32 = 0;
        let mut ty: u8 = 0;
        let ok = unsafe { f(captain_guid_low, &raw mut team_id, &raw mut ty) };
        if ok { Some((team_id, ty)) } else { None }
    }

    fn disband_arena_team(&self, team_id: u32) {
        if let Some(f) = self.cbs.disband_arena_team {
            unsafe { f(team_id) };
        }
    }

    fn get_player_arena_team_id_by_type(&self, guid_low: u32, team_type: u8) -> u32 {
        self.cbs
            .get_player_arena_team_id_by_type
            .map_or(0, |f| unsafe { f(guid_low, team_type) })
    }

    fn query_random_arena_team_name(&self) -> Option<(String, u8)> {
        let f = self.cbs.query_random_arena_team_name?;
        let mut buf = [0u8; 32];
        let mut ty: u8 = 0;
        let ok = unsafe {
            f(
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &raw mut ty,
            )
        };
        if !ok {
            return None;
        }
        let name = c_field_to_string(&buf);
        if name.is_empty() {
            None
        } else {
            Some((name, ty))
        }
    }

    fn create_arena_team(&self, captain_guid_low: u32, team_type: u8, name: &str) -> u32 {
        let Some(f) = self.cbs.create_arena_team else {
            return 0;
        };
        let buf = make_c_buf::<32>(name);
        unsafe { f(captain_guid_low, team_type, buf.as_ptr().cast()) }
    }

    fn add_arena_team_member(&self, team_id: u32, member_guid_low: u32) -> bool {
        self.cbs
            .add_arena_team_member
            .is_some_and(|f| unsafe { f(team_id, member_guid_low) })
    }

    fn get_arena_team_members_size(&self, team_id: u32) -> u32 {
        self.cbs
            .get_arena_team_members_size
            .map_or(0, |f| unsafe { f(team_id) })
    }

    fn set_arena_team_emblem(
        &self,
        team_id: u32,
        background_color: u32,
        style: u32,
        color: u32,
        border_style: u32,
        border_color: u32,
    ) {
        if let Some(f) = self.cbs.set_arena_team_emblem {
            unsafe {
                f(
                    team_id,
                    background_color,
                    style,
                    color,
                    border_style,
                    border_color,
                );
            };
        }
    }

    fn set_arena_team_rating(&self, team_id: u32, rating: u32) {
        if let Some(f) = self.cbs.set_arena_team_rating {
            unsafe { f(team_id, rating) };
        }
    }

    fn save_arena_team(&self, team_id: u32) {
        if let Some(f) = self.cbs.save_arena_team {
            unsafe { f(team_id) };
        }
    }

    fn log_debug(&self, msg: &str) {
        let Some(f) = self.cbs.log_debug else {
            return;
        };
        let buf = make_c_buf::<512>(msg);
        unsafe { f(buf.as_ptr().cast()) };
    }
}

/// Shared implementation for `query_name_pool` and
/// `query_existing_character_names` — they differ only in which
/// `RandomFactoryCallbacks` slot supplies the rows but use the same
/// `free_name_pool_list` for cleanup.
#[allow(unsafe_code)]
fn query_name_pool_inner(
    query: Option<unsafe extern "C" fn(*mut *mut BotNamePoolRow) -> u32>,
    free: Option<unsafe extern "C" fn(*mut BotNamePoolRow, u32)>,
) -> Vec<NamePoolRow> {
    let Some(query) = query else {
        return Vec::new();
    };
    let Some(free) = free else {
        return Vec::new();
    };
    let mut ptr: *mut BotNamePoolRow = core::ptr::null_mut();
    // Safety: C side allocates into *out_rows, we free via paired callback.
    let count = unsafe { query(&raw mut ptr) };
    if ptr.is_null() || count == 0 {
        return Vec::new();
    }
    // Safety: C guarantees `count` readable `BotNamePoolRow`s.
    let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
    let out: Vec<NamePoolRow> = slice
        .iter()
        .map(|r| {
            let name_bytes: [u8; 16] = r.name.map(|b| b as u8);
            NamePoolRow {
                gender_index: r.gender_index,
                name: c_field_to_string(&name_bytes),
                is_taken: r.is_taken,
            }
        })
        .collect();
    // Safety: paired free.
    unsafe { free(ptr, count) };
    out
}

/// In-memory mock used by the `crates/playerbot` random factory tests.
#[cfg(any(test, feature = "mock"))]
pub use mock_impl::{MockRandomFactoryEvent, MockRandomFactoryWorld};

#[cfg(any(test, feature = "mock"))]
mod mock_impl {
    use super::{
        BotDeleteEvent, CharAppearance, CharacterAccount, CreateParams, NamePoolRow,
        PlayerSnapshot, RandomFactoryWorld,
    };
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    /// Recorded event emitted by [`MockRandomFactoryWorld`]. Tests assert
    /// on the list after running the factory to verify transitions.
    #[derive(Clone, Debug, PartialEq)]
    pub enum MockRandomFactoryEvent {
        Urand {
            min: u32,
            max: u32,
        },
        QueryBotDeleteEvent,
        QueryAccountIdByName(String),
        QueryAccountIdsLikePrefix(String),
        CreateAccount {
            name: String,
            password: String,
            max_expansion: u8,
        },
        DeleteAccount(u32),
        GetCharacterCount(u32),
        QueryCharactersByAccount(u32),
        QueryFriendGuids,
        OnPlayerLoginError(u32),
        DeleteCharacterFromDb {
            guid_low: u32,
            account_id: u32,
        },
        PruneRandomBotsTable,
        QueryNamePool,
        QueryExistingCharacterNames,
        QueryRandomBotName(u8),
        QueryCharAppearance {
            race: u8,
            gender: u8,
        },
        CreateRandomBotCharacter(Box<CreateParams>),
        SaveAndLogoutAllOnline,
        QueryCharacterAccounts,
        GetGuildIdByLeader(u32),
        GetGuildLeaderAccountId(u32),
        DisbandGuild(u32),
        GetPlayerSnapshot(u32),
        QueryRandomGuildName,
        CreateRandomGuild {
            leader_guid_low: u32,
            name: String,
        },
        SetGuildEmblem {
            guild_id: u32,
            style: u32,
            color: u32,
            border_style: u32,
            border_color: u32,
            background_color: u32,
        },
        SetGuildInfo {
            guild_id: u32,
            info: String,
        },
        QueryRandomBotAddEvent,
        GetArenaTeamByCaptain(u32),
        DisbandArenaTeam(u32),
        GetPlayerArenaTeamIdByType {
            guid_low: u32,
            team_type: u8,
        },
        QueryRandomArenaTeamName,
        CreateArenaTeam {
            captain_guid_low: u32,
            team_type: u8,
            name: String,
        },
        AddArenaTeamMember {
            team_id: u32,
            member_guid_low: u32,
        },
        GetArenaTeamMembersSize(u32),
        SetArenaTeamEmblem {
            team_id: u32,
            background_color: u32,
            style: u32,
            color: u32,
            border_style: u32,
            border_color: u32,
        },
        SetArenaTeamRating {
            team_id: u32,
            rating: u32,
        },
        SaveArenaTeam(u32),
        Debug(String),
    }

    #[derive(Default)]
    struct MockState {
        /// Queued return values for `urand_range`. When empty, falls
        /// through to `min`.
        rng_queue: VecDeque<u32>,

        bot_delete_event: BotDeleteEvent,
        accounts_by_name: HashMap<String, u32>,
        accounts_by_prefix: HashMap<String, Vec<u32>>,
        character_count: HashMap<u32, u32>,
        characters_by_account: HashMap<u32, Vec<u32>>,
        friend_guids: Vec<u32>,
        name_pool: Vec<NamePoolRow>,
        existing_character_names: Vec<NamePoolRow>,
        random_bot_names: HashMap<u8, VecDeque<String>>,
        char_appearances: HashMap<(u8, u8), CharAppearance>,
        create_character_results: VecDeque<bool>,
        character_accounts: Vec<CharacterAccount>,
        guild_id_by_leader: HashMap<u32, u32>,
        guild_leader_accounts: HashMap<u32, u32>,
        player_snapshots: HashMap<u32, PlayerSnapshot>,
        random_guild_names: VecDeque<String>,
        create_guild_results: HashMap<u32, u32>,
        random_bot_add_guids: Vec<u32>,
        arena_team_by_captain: HashMap<u32, (u32, u8)>,
        arena_team_id_by_player_type: HashMap<(u32, u8), u32>,
        random_arena_team_names: VecDeque<(String, u8)>,
        create_arena_team_results: HashMap<u32, u32>,
        add_arena_team_member_results: VecDeque<bool>,
        arena_team_members_size: HashMap<u32, u32>,

        events: Vec<MockRandomFactoryEvent>,
    }

    /// Fixture-backed [`RandomFactoryWorld`] impl.
    ///
    /// Test code configures return values via the `set_*` and `push_*`
    /// helpers, runs the random bot factory through its orchestration
    /// entry points, and asserts on the recorded event log.
    #[derive(Default)]
    pub struct MockRandomFactoryWorld {
        state: Mutex<MockState>,
    }

    impl MockRandomFactoryWorld {
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Queue `value` as the next `urand_range(min, max)` return.
        pub fn push_rng(&self, value: u32) {
            self.state.lock().unwrap().rng_queue.push_back(value);
        }

        /// Queue a sequence of rng return values.
        pub fn push_rng_sequence(&self, values: impl IntoIterator<Item = u32>) {
            let mut s = self.state.lock().unwrap();
            for v in values {
                s.rng_queue.push_back(v);
            }
        }

        pub fn set_bot_delete_event(&self, event: BotDeleteEvent) {
            self.state.lock().unwrap().bot_delete_event = event;
        }

        pub fn set_account_id_by_name(&self, name: &str, account_id: u32) {
            self.state
                .lock()
                .unwrap()
                .accounts_by_name
                .insert(name.to_string(), account_id);
        }

        pub fn set_accounts_by_prefix(&self, prefix: &str, ids: Vec<u32>) {
            self.state
                .lock()
                .unwrap()
                .accounts_by_prefix
                .insert(prefix.to_string(), ids);
        }

        pub fn set_character_count(&self, account_id: u32, count: u32) {
            self.state
                .lock()
                .unwrap()
                .character_count
                .insert(account_id, count);
        }

        pub fn set_characters_by_account(&self, account_id: u32, guids: Vec<u32>) {
            self.state
                .lock()
                .unwrap()
                .characters_by_account
                .insert(account_id, guids);
        }

        pub fn set_friend_guids(&self, guids: Vec<u32>) {
            self.state.lock().unwrap().friend_guids = guids;
        }

        pub fn set_name_pool(&self, rows: Vec<NamePoolRow>) {
            self.state.lock().unwrap().name_pool = rows;
        }

        pub fn set_existing_character_names(&self, rows: Vec<NamePoolRow>) {
            self.state.lock().unwrap().existing_character_names = rows;
        }

        pub fn push_random_bot_name(&self, race_and_gender: u8, name: String) {
            self.state
                .lock()
                .unwrap()
                .random_bot_names
                .entry(race_and_gender)
                .or_default()
                .push_back(name);
        }

        pub fn set_char_appearance(&self, race: u8, gender: u8, appearance: CharAppearance) {
            self.state
                .lock()
                .unwrap()
                .char_appearances
                .insert((race, gender), appearance);
        }

        pub fn push_create_character_result(&self, ok: bool) {
            self.state
                .lock()
                .unwrap()
                .create_character_results
                .push_back(ok);
        }

        pub fn set_character_accounts(&self, rows: Vec<CharacterAccount>) {
            self.state.lock().unwrap().character_accounts = rows;
        }

        pub fn set_guild_id_by_leader(&self, leader: u32, guild_id: u32) {
            self.state
                .lock()
                .unwrap()
                .guild_id_by_leader
                .insert(leader, guild_id);
        }

        pub fn set_guild_leader_account_id(&self, guild_id: u32, account_id: u32) {
            self.state
                .lock()
                .unwrap()
                .guild_leader_accounts
                .insert(guild_id, account_id);
        }

        pub fn set_player_snapshot(&self, guid_low: u32, snapshot: PlayerSnapshot) {
            self.state
                .lock()
                .unwrap()
                .player_snapshots
                .insert(guid_low, snapshot);
        }

        pub fn push_random_guild_name(&self, name: String) {
            self.state
                .lock()
                .unwrap()
                .random_guild_names
                .push_back(name);
        }

        pub fn set_create_guild_result(&self, leader: u32, guild_id: u32) {
            self.state
                .lock()
                .unwrap()
                .create_guild_results
                .insert(leader, guild_id);
        }

        pub fn set_random_bot_add_guids(&self, guids: Vec<u32>) {
            self.state.lock().unwrap().random_bot_add_guids = guids;
        }

        pub fn set_arena_team_by_captain(&self, captain: u32, team_id: u32, team_type: u8) {
            self.state
                .lock()
                .unwrap()
                .arena_team_by_captain
                .insert(captain, (team_id, team_type));
        }

        pub fn set_player_arena_team_id(&self, guid_low: u32, team_type: u8, team_id: u32) {
            self.state
                .lock()
                .unwrap()
                .arena_team_id_by_player_type
                .insert((guid_low, team_type), team_id);
        }

        pub fn push_random_arena_team_name(&self, name: String, team_type: u8) {
            self.state
                .lock()
                .unwrap()
                .random_arena_team_names
                .push_back((name, team_type));
        }

        pub fn set_create_arena_team_result(&self, captain: u32, team_id: u32) {
            self.state
                .lock()
                .unwrap()
                .create_arena_team_results
                .insert(captain, team_id);
        }

        pub fn push_add_arena_team_member_result(&self, ok: bool) {
            self.state
                .lock()
                .unwrap()
                .add_arena_team_member_results
                .push_back(ok);
        }

        pub fn set_arena_team_members_size(&self, team_id: u32, size: u32) {
            self.state
                .lock()
                .unwrap()
                .arena_team_members_size
                .insert(team_id, size);
        }

        pub fn take_events(&self) -> Vec<MockRandomFactoryEvent> {
            std::mem::take(&mut self.state.lock().unwrap().events)
        }

        pub fn events(&self) -> Vec<MockRandomFactoryEvent> {
            self.state.lock().unwrap().events.clone()
        }
    }

    impl RandomFactoryWorld for MockRandomFactoryWorld {
        fn urand_range(&self, min: u32, max: u32) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::Urand { min, max });
            s.rng_queue.pop_front().unwrap_or(min)
        }

        fn query_bot_delete_event(&self) -> BotDeleteEvent {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::QueryBotDeleteEvent);
            s.bot_delete_event
        }

        fn query_account_id_by_name(&self, name: &str) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::QueryAccountIdByName(name.to_string()));
            s.accounts_by_name.get(name).copied().unwrap_or(0)
        }

        fn query_account_ids_like_prefix(&self, prefix: &str) -> Vec<u32> {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::QueryAccountIdsLikePrefix(
                    prefix.to_string(),
                ));
            s.accounts_by_prefix
                .get(prefix)
                .cloned()
                .unwrap_or_default()
        }

        fn create_account(&self, name: &str, password: &str, max_expansion: u8) {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::CreateAccount {
                name: name.to_string(),
                password: password.to_string(),
                max_expansion,
            });
        }

        fn delete_account(&self, account_id: u32) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::DeleteAccount(account_id));
        }

        fn get_character_count(&self, account_id: u32) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::GetCharacterCount(account_id));
            s.character_count.get(&account_id).copied().unwrap_or(0)
        }

        fn query_characters_by_account(&self, account_id: u32) -> Vec<u32> {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::QueryCharactersByAccount(account_id));
            s.characters_by_account
                .get(&account_id)
                .cloned()
                .unwrap_or_default()
        }

        fn query_friend_guids(&self) -> Vec<u32> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::QueryFriendGuids);
            s.friend_guids.clone()
        }

        fn on_player_login_error(&self, guid_low: u32) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::OnPlayerLoginError(guid_low));
        }

        fn delete_character_from_db(&self, guid_low: u32, account_id: u32) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::DeleteCharacterFromDb {
                    guid_low,
                    account_id,
                });
        }

        fn prune_random_bots_table(&self) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::PruneRandomBotsTable);
        }

        fn query_name_pool(&self) -> Vec<NamePoolRow> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::QueryNamePool);
            s.name_pool.clone()
        }

        fn query_existing_character_names(&self) -> Vec<NamePoolRow> {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::QueryExistingCharacterNames);
            s.existing_character_names.clone()
        }

        fn query_random_bot_name(&self, race_and_gender: u8) -> Option<String> {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::QueryRandomBotName(race_and_gender));
            s.random_bot_names
                .get_mut(&race_and_gender)
                .and_then(VecDeque::pop_front)
        }

        fn query_char_appearance(&self, race: u8, gender: u8) -> Option<CharAppearance> {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::QueryCharAppearance { race, gender });
            s.char_appearances.get(&(race, gender)).copied()
        }

        fn create_random_bot_character(&self, params: &CreateParams) -> bool {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::CreateRandomBotCharacter(Box::new(
                    params.clone(),
                )));
            s.create_character_results.pop_front().unwrap_or(true)
        }

        fn save_and_logout_all_online(&self) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::SaveAndLogoutAllOnline);
        }

        fn query_character_accounts(&self) -> Vec<CharacterAccount> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::QueryCharacterAccounts);
            s.character_accounts.clone()
        }

        fn get_guild_id_by_leader(&self, leader_guid_low: u32) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::GetGuildIdByLeader(leader_guid_low));
            s.guild_id_by_leader
                .get(&leader_guid_low)
                .copied()
                .unwrap_or(0)
        }

        fn get_guild_leader_account_id(&self, guild_id: u32) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::GetGuildLeaderAccountId(guild_id));
            s.guild_leader_accounts.get(&guild_id).copied().unwrap_or(0)
        }

        fn disband_guild(&self, guild_id: u32) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::DisbandGuild(guild_id));
        }

        fn get_player_snapshot(&self, guid_low: u32) -> Option<PlayerSnapshot> {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::GetPlayerSnapshot(guid_low));
            s.player_snapshots.get(&guid_low).cloned()
        }

        fn query_random_guild_name(&self) -> Option<String> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::QueryRandomGuildName);
            s.random_guild_names.pop_front()
        }

        fn create_random_guild(&self, leader_guid_low: u32, name: &str) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::CreateRandomGuild {
                leader_guid_low,
                name: name.to_string(),
            });
            s.create_guild_results
                .get(&leader_guid_low)
                .copied()
                .unwrap_or(0)
        }

        fn set_guild_emblem(
            &self,
            guild_id: u32,
            style: u32,
            color: u32,
            border_style: u32,
            border_color: u32,
            background_color: u32,
        ) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::SetGuildEmblem {
                    guild_id,
                    style,
                    color,
                    border_style,
                    border_color,
                    background_color,
                });
        }

        fn set_guild_info(&self, guild_id: u32, info: &str) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::SetGuildInfo {
                    guild_id,
                    info: info.to_string(),
                });
        }

        fn query_random_bot_add_event(&self) -> Vec<u32> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::QueryRandomBotAddEvent);
            s.random_bot_add_guids.clone()
        }

        fn get_arena_team_by_captain(&self, captain_guid_low: u32) -> Option<(u32, u8)> {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::GetArenaTeamByCaptain(
                    captain_guid_low,
                ));
            s.arena_team_by_captain.get(&captain_guid_low).copied()
        }

        fn disband_arena_team(&self, team_id: u32) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::DisbandArenaTeam(team_id));
        }

        fn get_player_arena_team_id_by_type(&self, guid_low: u32, team_type: u8) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::GetPlayerArenaTeamIdByType {
                    guid_low,
                    team_type,
                });
            s.arena_team_id_by_player_type
                .get(&(guid_low, team_type))
                .copied()
                .unwrap_or(0)
        }

        fn query_random_arena_team_name(&self) -> Option<(String, u8)> {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::QueryRandomArenaTeamName);
            s.random_arena_team_names.pop_front()
        }

        fn create_arena_team(
            &self,
            captain_guid_low: u32,
            team_type: u8,
            name: &str,
        ) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::CreateArenaTeam {
                captain_guid_low,
                team_type,
                name: name.to_string(),
            });
            s.create_arena_team_results
                .get(&captain_guid_low)
                .copied()
                .unwrap_or(0)
        }

        fn add_arena_team_member(&self, team_id: u32, member_guid_low: u32) -> bool {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockRandomFactoryEvent::AddArenaTeamMember {
                team_id,
                member_guid_low,
            });
            s.add_arena_team_member_results
                .pop_front()
                .unwrap_or(true)
        }

        fn get_arena_team_members_size(&self, team_id: u32) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events
                .push(MockRandomFactoryEvent::GetArenaTeamMembersSize(team_id));
            s.arena_team_members_size
                .get(&team_id)
                .copied()
                .unwrap_or(0)
        }

        fn set_arena_team_emblem(
            &self,
            team_id: u32,
            background_color: u32,
            style: u32,
            color: u32,
            border_style: u32,
            border_color: u32,
        ) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::SetArenaTeamEmblem {
                    team_id,
                    background_color,
                    style,
                    color,
                    border_style,
                    border_color,
                });
        }

        fn set_arena_team_rating(&self, team_id: u32, rating: u32) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::SetArenaTeamRating { team_id, rating });
        }

        fn save_arena_team(&self, team_id: u32) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::SaveArenaTeam(team_id));
        }

        fn log_debug(&self, msg: &str) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockRandomFactoryEvent::Debug(msg.to_string()));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn urand_drains_queue_then_min() {
            let m = MockRandomFactoryWorld::new();
            m.push_rng_sequence([5, 7]);
            assert_eq!(m.urand_range(0, 100), 5);
            assert_eq!(m.urand_range(0, 100), 7);
            // Queue drained — falls through to min.
            assert_eq!(m.urand_range(3, 100), 3);
        }

        #[test]
        fn bot_delete_event_round_trip() {
            let m = MockRandomFactoryWorld::new();
            assert_eq!(m.query_bot_delete_event(), BotDeleteEvent::default());
            m.set_bot_delete_event(BotDeleteEvent {
                scheduled: true,
                delete_friends: true,
            });
            let ev = m.query_bot_delete_event();
            assert!(ev.scheduled);
            assert!(ev.delete_friends);
        }

        #[test]
        fn name_pool_round_trip() {
            let m = MockRandomFactoryWorld::new();
            m.set_name_pool(vec![NamePoolRow {
                gender_index: 0,
                name: "Hodor".to_string(),
                is_taken: false,
            }]);
            let pool = m.query_name_pool();
            assert_eq!(pool.len(), 1);
            assert_eq!(pool[0].name, "Hodor");
            assert!(matches!(
                m.events().as_slice(),
                [MockRandomFactoryEvent::QueryNamePool]
            ));
        }

        #[test]
        fn random_bot_name_is_consumed() {
            let m = MockRandomFactoryWorld::new();
            m.push_random_bot_name(3, "Arthas".to_string());
            assert_eq!(m.query_random_bot_name(3).as_deref(), Some("Arthas"));
            assert_eq!(m.query_random_bot_name(3), None);
            // Unknown gender index also returns None.
            assert_eq!(m.query_random_bot_name(9), None);
        }

        #[test]
        fn create_random_bot_character_defaults_to_true() {
            let m = MockRandomFactoryWorld::new();
            let params = CreateParams {
                account_id: 1,
                race: 1,
                cls: 1,
                gender: 0,
                name: "Uther".to_string(),
                skin_color: 0,
                face_id: 0,
                face_color: 0,
                hair_style: 0,
                hair_color: 0,
                facial_hair: 0,
            };
            assert!(m.create_random_bot_character(&params));
            m.push_create_character_result(false);
            assert!(!m.create_random_bot_character(&params));
        }

        #[test]
        fn events_are_recorded_in_order() {
            let m = MockRandomFactoryWorld::new();
            m.query_bot_delete_event();
            m.create_account("rndbot0001", "rndbot0001", 0);
            m.save_and_logout_all_online();
            let events = m.take_events();
            assert_eq!(events.len(), 3);
            assert!(matches!(events[0], MockRandomFactoryEvent::QueryBotDeleteEvent));
            assert!(matches!(events[1], MockRandomFactoryEvent::CreateAccount { .. }));
            assert!(matches!(
                events[2],
                MockRandomFactoryEvent::SaveAndLogoutAllOnline
            ));
            assert!(m.take_events().is_empty());
        }
    }
}
