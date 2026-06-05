// Trait body imported verbatim from the legacy `BotInterface`. Two lints
// trip on the imported text and aren't worth a stylistic rewrite of every
// doc comment / multi-arg method signature in the trait.
#![allow(clippy::doc_markdown, clippy::too_many_arguments)]
#![forbid(unsafe_code)]

//! The `World` trait — Rust abstraction over the C `BotCallbacks` vtable.
//!
//! Production code uses `VtableWorld` (wraps the C function pointer table).
//! Tests use `MockWorld` (in-memory mock that records all commands issued).
//!
//! Behaviour-tree nodes and the tick context hold `&dyn World`, so the same
//! AI code path runs in both contexts without conditional compilation.

use crate::{
    BotAuraInfo, BotMailSummary, BotPosition, BotRole, BotSpellInfo, BotUnitSnapshot,
    BotWorldSnapshot, ItemId, SpellId, UnitHandle,
    owned::{
        AuraList, BotSpellList, GatherableList, InventoryList, OwnedList, QuestLog,
        ReputationList, SkillList, TalentList, TaxiNodeList, ThreatList, TravelDestList,
        UnitList,
    },
};

/// Summary data pulled from `Guild*` for `PlayerbotFactory::InitGuild`.
///
/// Backs `World::factory_query_guild_summary`. The C++ side looks up the
/// guild by id, reads the leader's team via `sObjectMgr.GetPlayerTeamByGUID`,
/// grabs the live member count, parses the `GINFO` string as a cap, and
/// returns the guild name for logging.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuildSummary {
    /// Team of the guild leader. 0 = Alliance, 1 = Horde.
    pub leader_team: u8,
    /// `guild->GetMemberSize()`.
    pub member_size: u32,
    /// `atoi(guild->GetGINFO().c_str())`. 0 when the GINFO string is empty
    /// or non-numeric — `PlayerbotFactory::InitGuild` falls back to
    /// `urand(10, 15)` in that case.
    pub max_members_hint: u32,
    /// `guild->GetName()` — captured for log output.
    pub name: String,
}

/// The complete interface a bot has to the game world.
///
/// List-returning methods hand back an `OwnedList<'_, T>` (RAII guard around
/// the C-allocated buffer); scalar methods return owned `T`. The `Send` bound
/// is retained for now; the migration plan flips to `!Send` in a later phase.
pub trait World: Send {
    /* ── State snapshot ──────────────────────────────────────────────── */

    /// Read the full bot+group snapshot for this tick. Call once per tick.
    fn get_snapshot(&self) -> BotWorldSnapshot;

    /// Read a specific unit's snapshot (group member, nearby enemy, boss).
    fn get_unit_snapshot(&self, target: UnitHandle) -> BotUnitSnapshot;

    /* ── Aura queries ────────────────────────────────────────────────── */

    fn has_aura(&self, unit: UnitHandle, spell_id: SpellId) -> bool;
    fn get_aura(&self, unit: UnitHandle, spell_id: SpellId) -> Option<BotAuraInfo>;
    /// All auras on `unit`. Used for encounter phase detection and debuff tracking.
    fn get_auras(&self, unit: UnitHandle) -> AuraList<'_>;

    /* ── Threat queries ──────────────────────────────────────────────── */

    /// Full threat list on `target_unit` (e.g. boss), ordered highest→lowest.
    fn get_threat_list(&self, target_unit: UnitHandle) -> ThreatList<'_>;
    /// Threat that `from_unit` has on `target_unit`.
    fn get_unit_threat(&self, target_unit: UnitHandle, from_unit: UnitHandle) -> f32;

    /* ── Unit queries ────────────────────────────────────────────────── */

    fn unit_distance(&self, target: UnitHandle) -> f32;
    fn can_cast(&self, spell_id: SpellId, target: UnitHandle) -> bool;
    fn spell_cooldown_ms(&self, spell_id: SpellId) -> u32;
    fn has_los(&self, target: UnitHandle) -> bool;
    fn get_nearby_units(&self, range: f32, hostile: bool) -> UnitList<'_>;
    /// Units that have this bot on their threat list (actual attackers).
    /// Unlike `get_nearby_units(hostile=true)`, only returns mobs fighting the bot.
    fn get_attackers(&self) -> UnitList<'_> {
        OwnedList::empty()
    }
    /// True if the bot is currently positioned in `target`'s rear arc.
    /// Used to gate abilities like Backstab that require being behind.
    fn bot_is_behind(&self, _target: UnitHandle) -> bool {
        false
    }
    /// True when the bot is on `target`'s flank — outside both the front
    /// cleave/breath cone and the rear arc. Used to position melee at the
    /// side of tail-sweeping dragons (Onyxia, Nefarian, …) where "behind"
    /// is the tail-sweep zone.
    fn bot_is_at_flank(&self, _target: UnitHandle) -> bool {
        false
    }
    /// ItemPrototype.SubClass of the weapon in `slot` (0=mainhand, 1=offhand,
    /// 2=ranged), or `u32::MAX` when empty / non-weapon. Values match
    /// `ITEM_SUBCLASS_WEAPON_*` (dagger=15, sword=7, etc.).
    fn bot_equipped_weapon_subclass(&self, _slot: u8) -> u32 {
        u32::MAX
    }
    /// Total count of `item_id` in the bot's inventory (bags, not bank).
    fn bot_item_count(&self, _item_id: ItemId) -> u32 {
        0
    }
    /// Bitmask of active totem slots (bit 0=fire, 1=earth, 2=water, 3=air).
    fn bot_active_totem_mask(&self) -> u8 {
        0
    }
    /// True if the weapon in `slot` (0=main, 1=off) has a temporary enchant.
    fn bot_weapon_enchanted(&self, _slot: u8) -> bool {
        false
    }
    /// Bitmask of ready death-knight rune slots (`WotLK` only; bits 0–5).
    fn bot_runes_ready_mask(&self) -> u8 {
        0
    }
    /// True if the bot has learned `spell_id` (`Player::HasSpell`). Does not
    /// check cooldown or power cost — use `can_cast` for full castability.
    /// Default `true` so existing mocks stay happy; real impl calls C.
    fn knows_spell(&self, _spell_id: SpellId) -> bool {
        true
    }
    /// Look up the spell name from the server spell DB. Returns an empty
    /// string if the spell does not exist. Used by the debug monitor for
    /// human-readable logging.
    fn get_spell_name(&self, _spell_id: SpellId) -> String {
        String::new()
    }

    /* ── Pathfinding / positioning ───────────────────────────────────── */

    /// Position directly behind `target` at `distance` yards (cleave avoidance).
    fn get_behind_position(&self, target: UnitHandle, distance: f32) -> BotPosition;
    /// Nearest reachable position not in a ground hazard within `search_radius` yards.
    fn get_safe_position(&self, search_radius: f32) -> Option<BotPosition>;
    /// Spread position: this bot is index `idx` of `total` bots spreading at `radius` around `center`.
    fn get_spread_position(
        &self,
        center: UnitHandle,
        radius: f32,
        idx: u8,
        total: u8,
    ) -> BotPosition;
    /// Returns true if the bot has line-of-sight to (x, y, z).
    fn can_reach(&self, x: f32, y: f32, z: f32) -> bool;
    /// Returns true if the bot can reach (x, y, z) via navmesh pathfinding
    /// (not just line-of-sight). More expensive than `can_reach` but
    /// respects walls, terrain boundaries, and obstacles.
    fn can_pathfind_to(&self, x: f32, y: f32, z: f32) -> bool {
        // Default: fall back to LoS check for test mocks.
        self.can_reach(x, y, z)
    }

    /* ── Commands ────────────────────────────────────────────────────── */

    fn cast_spell(&self, spell_id: SpellId, target: UnitHandle) -> bool;
    fn cast_spell_pos(&self, spell_id: SpellId, x: f32, y: f32, z: f32) -> bool;
    fn move_to(&self, x: f32, y: f32, z: f32) -> bool;
    fn follow(&self, target: UnitHandle, dist: f32, angle: f32) -> bool;
    /// Chase a unit in combat using `MoveChase` — smoothly tracks the target
    /// at the given distance and angle without restarting splines every tick.
    /// Default falls back to `follow` so test mocks don't need updating.
    fn chase(&self, target: UnitHandle, dist: f32, angle: f32) -> bool {
        self.follow(target, dist, angle)
    }
    fn stop_moving(&self) -> bool;
    /// Set the bot's facing angle (radians). Stops movement and sends a
    /// heartbeat so the server acknowledges the orientation immediately.
    fn set_facing(&self, _angle: f32) {}
    fn attack(&self, target: UnitHandle) -> bool;
    fn auto_attack(&self, enable: bool) -> bool;
    /// Ranged auto-attack / wand-shoot pull. Inspects the bot's ranged
    /// slot and fires the appropriate spell (Auto Shot 75 for
    /// bow/gun/crossbow, Shoot 5019 for wand) at `target`. Returns
    /// false when the bot has no ranged weapon, doesn't know the
    /// matching spell, or the cast fails. Used by `Bt::PullTarget`
    /// as its first-choice pull path; callers should fall back to
    /// `cast_spell` / `taunt` / `attack` when this returns `false`.
    fn auto_shoot(&self, _target: UnitHandle) -> bool {
        false
    }
    fn say(&self, msg: &str, lang: u32) -> bool;
    /// Whisper a message directly to a specific player (`target_guid`).
    /// Used for per-command replies to the sender.
    fn whisper(&self, _target_guid: u64, _msg: &str) -> bool {
        false
    }
    /// PB2-style `TellPlayerNoFacing` routing: broadcasts the reply to the
    /// bot's PARTY/RAID channel when it is in a group, otherwise whispers
    /// the sender. Used by [`crate::commands::reply`] so bots respond in
    /// whatever channel PB2 would have — group chat for group members,
    /// whisper for solo bots answering a stranger.
    fn tell_player(&self, _target_guid: u64, _msg: &str) -> bool {
        false
    }
    /// Addon-channel reply: send a message back over `CHAT_MSG_ADDON` /
    /// `LANG_ADDON` to the requester so the Mangosbot / `RaidControl` UI's
    /// addon-message listener consumes it. Used when the incoming command
    /// arrived via the addon wire (`#a` prefix, `SendAddonMessage("BOT",…)`,
    /// or the `debug …` shortcut). Mirrors PB2 `PlayerbotAI.cpp:3475-3485`.
    fn tell_addon(&self, _target_guid: u64, _msg: &str) -> bool {
        false
    }
    /// Broadcast an addon message to the bot's group/raid. Wire format is
    /// `"PREFIX\tBODY"` — identical to the client's `SendAddonMessage()`.
    /// Used for protocols like KLHThreatMeter.
    fn send_group_addon(&self, _prefix: &str, _msg: &str) -> bool {
        false
    }
    fn use_item(&self, item_id: ItemId, target: UnitHandle) -> bool;
    fn taunt(&self, target: UnitHandle) -> bool;

    /// Teleport the bot to an absolute world position. Used by the
    /// `summon` chat command to snap the bot to the requester.
    fn teleport_to(&self, _map_id: u32, _x: f32, _y: f32, _z: f32, _o: f32) -> bool {
        false
    }

    /// Resolve a player GUID to its current world position. Returns
    /// `None` if the player is offline or not found. Used by the
    /// `summon` command to look up the requester's location.
    fn get_player_position(&self, _player_guid: u64) -> Option<BotPosition> {
        None
    }

    /// Full-fat summon: mirrors PB2 `SummonAction::Teleport` exactly
    /// (angle search + LOS check around the requester, in-place revive
    /// if the bot is dead, motion-master clear, teleport, transport
    /// re-parenting). Returns true when the bot was successfully moved.
    fn summon_to_player(&self, _requester_guid: u64) -> bool {
        false
    }

    /// Find the nearest hostile unit currently marked with the given raid
    /// target icon (1 = star, 2 = circle, 3 = diamond, 4 = triangle,
    /// 5 = moon, 6 = square, 7 = cross, 8 = skull). Returns `None` if
    /// no unit is marked with that icon in range.
    fn get_unit_with_raid_icon(&self, icon: u8) -> Option<UnitHandle> {
        let _ = icon;
        None
    }

    /// Assign raid target icon `icon` (0..=7, raw `Group::SetTargetIcon`
    /// indexing — star=0, circle=1, diamond=2, triangle=3, moon=4,
    /// square=5, cross=6, skull=7) to `target`. Broadcasts
    /// `MSG_RAID_TARGET_UPDATE` to every group member so Mangosbot
    /// marker UI and other clients redraw. Returns `false` when the
    /// bot is ungrouped, the icon is out of range, or the target
    /// cannot be resolved. Passing `target = 0` clears the icon.
    /// Used by `Bt::MarkRti` / `Bt::MarkRtiCc`. GOTCHA: this uses
    /// 0..=7 indexing, while the existing `get_unit_with_raid_icon`
    /// uses 1..=8 — the BT handlers translate at the boundary.
    fn group_set_target_icon(&self, _target: UnitHandle, _icon: u8) -> bool {
        false
    }

    /* ── Group / raid ────────────────────────────────────────────────── */

    fn group_get_tank(&self) -> Option<UnitHandle>;
    fn group_get_healer(&self) -> Option<UnitHandle>;
    fn group_get_role(&self, member: UnitHandle) -> BotRole;

    /* ── Death / resurrection ───────────────────────────────────────── */

    fn accept_resurrect(&self) -> bool {
        false
    }
    fn get_corpse_position(&self) -> Option<BotPosition> {
        None
    }
    fn use_spirit_healer(&self) -> bool {
        false
    }
    /// In-place revive at full HP — used by the `summon` command when the
    /// bot is being teleported while dead, mirroring PB2's
    /// `SummonAction::Teleport` behavior (revive → spawn corpse bones →
    /// teleport). Returns false if the bot is already alive.
    fn resurrect_self(&self) -> bool {
        false
    }

    /* ── Mount ──────────────────────────────────────────────────────── */

    fn is_mounted(&self) -> bool {
        false
    }
    fn mount_up(&self) -> bool {
        false
    }
    fn dismount(&self) -> bool {
        false
    }
    fn is_indoor(&self) -> bool {
        false
    }

    /* ── Loot ───────────────────────────────────────────────────────── */

    fn get_nearby_lootable(&self, _range: f32) -> UnitList<'_> {
        OwnedList::empty()
    }
    fn open_loot(&self, _target: UnitHandle) -> bool {
        false
    }
    fn take_all_loot(&self) -> bool {
        false
    }

    /* ── NPC interaction ────────────────────────────────────────────── */

    fn get_nearby_npcs(&self, _range: f32, _npc_flags: u32) -> UnitList<'_> {
        OwnedList::empty()
    }
    fn interact_npc(&self, _npc: UnitHandle) -> bool {
        false
    }
    fn repair_all(&self) -> bool {
        false
    }
    fn sell_grey_items(&self) -> bool {
        false
    }
    fn has_sellable_items(&self) -> bool {
        false
    }
    fn get_durability_pct(&self) -> f32 {
        1.0
    }

    /* ── Quest ──────────────────────────────────────────────────────── */

    fn get_quest_log(&self) -> QuestLog<'_> {
        OwnedList::empty()
    }
    fn accept_all_quests(&self, _npc: UnitHandle) -> bool {
        false
    }
    fn turn_in_quest(&self, _npc: UnitHandle, _quest_id: u32) -> bool {
        false
    }
    /// Use a nearby gameobject that satisfies a "use object" objective of one
    /// of the bot's incomplete quests. Returns true if one was used.
    fn use_nearby_quest_object(&self, _range: f32) -> bool {
        false
    }
    /// True if `entry` is a creature the bot still needs to kill for an
    /// incomplete quest objective.
    fn is_quest_objective_creature(&self, _entry: u32) -> bool {
        false
    }

    /// Handle of a nearby creature actively escorting the bot for an
    /// incomplete escort quest, or `0`. The bot follows it so the escort
    /// keeps progressing; combat handles ambushes.
    fn active_escort_npc(&self) -> UnitHandle {
        0
    }

    /// Say a random idle "suggestion" in the bot's General chat channel
    /// (content from `ai_playerbot_texts`). The caller throttles this. Returns
    /// true if something was broadcast.
    fn bot_broadcast_random(&self) -> bool {
        false
    }

    /// Greet a nearby real (non-bot) player the bot hasn't greeted recently
    /// with a `/say` hello. The caller throttles this. Returns true if a
    /// greeting was said.
    fn bot_greet_nearby_player(&self) -> bool {
        false
    }

    /* ── Unit queries (extended) ────────────────────────────────────── */

    fn is_attackable(&self, _target: UnitHandle) -> bool {
        false
    }
    fn get_unit_level(&self, _target: UnitHandle) -> u8 {
        0
    }
    fn is_casting_interruptible(&self, _target: UnitHandle) -> bool {
        false
    }
    /// Coarse category for `target`:
    /// 0 = other/unknown, 1 = player, 2 = pet, 3 = critter.
    fn unit_kind(&self, _target: UnitHandle) -> u8 {
        0
    }

    /* ── Pet management ─────────────────────────────────────────────── */

    fn has_pet(&self) -> bool {
        false
    }
    fn pet_is_alive(&self) -> bool {
        false
    }
    fn pet_happiness(&self) -> u8 {
        3
    } // 1=unhappy, 2=content, 3=happy
    fn pet_health_pct(&self) -> u8 {
        0
    } // 0..100, 0 if no pet or dead
    fn summon_pet(&self) -> bool {
        false
    }
    fn revive_pet(&self) -> bool {
        false
    }
    fn feed_pet(&self) -> bool {
        false
    }
    /// Command the bot's pet to attack `target` (PB2 AttackAction). Returns
    /// false if the pet is passive/absent or already on that victim.
    fn pet_attack(&self, _target: UnitHandle) -> bool {
        false
    }
    /// Command the bot's pet to cast `spell_id` on `target` (e.g. Felhunter
    /// Spell Lock). Returns false if the pet is absent/dead, doesn't know the
    /// spell, it's on cooldown, or `CheckCast` rejects it.
    fn cast_pet_spell(&self, _spell_id: SpellId, _target: UnitHandle) -> bool {
        false
    }

    /* ── Dispel / party aura queries ────────────────────────────────── */

    /// Find a group member with a dispellable debuff that this bot can remove.
    /// Returns (`member_handle`, `debuff_spell_id`).
    ///
    /// `dispel_mask` restricts the search to specific schools. It's a
    /// bitmask over `DispelType` values — bit 1 = magic, bit 2 = curse,
    /// bit 3 = disease, bit 4 = poison (matching `1 << DispelType`).
    /// Passing `0` means "any school the bot can dispel" (the PB2
    /// `DispelTrigger` default).
    fn find_dispellable_target(&self, _dispel_mask: u8) -> Option<(UnitHandle, SpellId)> {
        None
    }

    /// Find a dead group member that can be resurrected.
    fn find_dead_party_member(&self) -> Option<UnitHandle> {
        None
    }

    /// Return the item id of the first usable potion in the bot's bags
    /// matching `category`, or `ItemId(0)` if none. `category`: 0 = buff
    /// potion (stat/damage elixirs), 1 = utility potion (free action,
    /// invulnerability, swiftness). Healing/mana potions are not covered
    /// here — they're selected via `factory_pick_potion_for_level`.
    fn find_potion_in_bags(&self, _category: u8) -> ItemId {
        ItemId(0)
    }

    /// Find the best food or drink in the bot's bags for its level.
    /// `category`: 11 = food (HP regen), 59 = drink (mana regen).
    /// Returns `ItemId(0)` if nothing suitable found.
    fn find_food_drink_in_bags(&self, _category: u32) -> ItemId {
        ItemId(0)
    }

    /// True when the bot's shared potion item cooldown (category 4) is
    /// ready. Cheap gate to run before `UseBuffPotion` action leaves
    /// enter the cast path. Defaults to `true` so stubs don't block
    /// strategies that never actually query it.
    fn potion_cooldown_ready(&self) -> bool {
        true
    }

    /// Activate the trinket equipped in `slot` (0 = top trinket /
    /// `EQUIPMENT_SLOT_TRINKET1`, 1 = bottom trinket /
    /// `EQUIPMENT_SLOT_TRINKET2`). Resolves the item slot, walks its
    /// `OnUse` spell list, and fires the first ready one. Returns `false`
    /// when the slot is empty, the item has no `OnUse` effect, every `OnUse`
    /// spell is on cooldown, or the cast fails. Used by the
    /// `Bt::UseTrinket(slot)` BT leaf (11h). Default stub returns
    /// `false` so existing mocks stay happy.
    fn use_trinket(&self, _slot: u8) -> bool {
        false
    }

    /* ── Social / group actions (11i) ──────────────────────────────────── */

    /// Accept a pending group/raid invitation. Returns false when no
    /// invite is pending.
    fn accept_group_invite(&self) -> bool {
        false
    }
    /// Leave the bot's current group/raid.
    fn leave_group(&self) -> bool {
        false
    }
    /// Accept a pending ready check.
    fn accept_ready_check(&self) -> bool {
        false
    }
    /// Accept a pending trade window.
    fn accept_trade(&self) -> bool {
        false
    }
    /// Accept an incoming duel request (`duel_state` == 1).
    fn accept_duel(&self) -> bool {
        false
    }
    /// Decline an incoming duel request.
    fn decline_duel(&self) -> bool {
        false
    }
    /// Accept a pending warlock/meeting-stone summon.
    fn accept_summon(&self) -> bool {
        false
    }
    /// Interact with a nearby meeting stone and queue for summoning.
    fn use_meeting_stone(&self) -> bool {
        false
    }

    /* ── PvP / duel / faction (11d) ──────────────────────────────────── */

    /// True when the bot currently has the `PvP` flag set
    /// (`Player::IsPvP`). Used by `PvpFlagged` BT condition to gate
    /// PvP-only strategies.
    fn is_pvp_flagged(&self) -> bool {
        false
    }

    /// Encoded duel state: `0` = no active duel, `1` = challenged /
    /// countdown (request pending, fight hasn't started), `2` = in
    /// progress (`DuelInfo::startTime > 0`). Direction (sender vs
    /// receiver) is intentionally collapsed — strategies only care
    /// whether a fight is about to happen or already happening.
    fn duel_state(&self) -> u8 {
        0
    }

    /// The bot's reputation rank with `faction_id`, as `ReputationRank`
    /// (0=hated .. 7=exalted). Returns `3` (neutral) when the bot has
    /// no record for the faction — mirrors the client default — and
    /// `255` when the faction id doesn't exist in the DBC.
    fn reputation_rank(&self, _faction_id: u32) -> u8 {
        3
    }

    /* ── Battleground ───────────────────────────────────────────────── */

    /// True if the bot is currently in a battleground instance.
    fn is_in_battleground(&self) -> bool {
        false
    }
    /// BG type: 1=AV, 2=WSG, 3=AB. 0 if not in BG.
    fn battleground_type(&self) -> u8 {
        0
    }
    /// Get the position of a nearby capturable BG objective (flag, base).
    fn get_bg_objective(&self) -> Option<BotPosition> {
        None
    }
    /// Interact with a BG objective (pick up flag, capture base).
    fn capture_bg_objective(&self) -> bool {
        false
    }
    /// Get nearby enemy players within range.
    fn get_nearby_enemies(&self, _range: f32) -> UnitList<'_> {
        OwnedList::empty()
    }

    /* ── RPG / social ───────────────────────────────────────────────── */

    /// Get a random navigable point near the bot within range.
    fn get_random_point_nearby(&self, _range: f32) -> Option<BotPosition> {
        None
    }
    /// Play an emote (wave, bow, dance, etc.).
    fn emote(&self, _emote_id: u32) -> bool {
        false
    }
    /// Get nearby friendly NPCs to gossip with (non-vendor, non-quest).
    fn get_nearby_gossip_npcs(&self, _range: f32) -> UnitList<'_> {
        OwnedList::empty()
    }

    /* ── Gathering (mining, herbalism, skinning) ────────────────────── */

    /// True if the bot has any gathering profession (mining, herbalism, skinning).
    fn has_gathering_skill(&self) -> bool {
        false
    }
    /// Get nearby gatherable game objects (ore veins, herb nodes) or skinnable corpses.
    fn get_nearby_gatherables(&self, _range: f32) -> GatherableList<'_> {
        OwnedList::empty()
    }
    /// Interact with a gatherable node/corpse to gather it.
    fn gather_node(&self, _handle: u64) -> bool {
        false
    }
    /// Distance to a game object handle.
    fn gameobject_distance(&self, _handle: u64) -> f32 {
        f32::MAX
    }
    /// Position of a game object handle.
    fn gameobject_position(&self, _handle: u64) -> BotPosition {
        BotPosition::default()
    }
    /// Find the nearest spawned `GameObject` with `entry` within `range` yards.
    /// Returns the packed GUID, or `None` if no matching GO is in range.
    /// Used by instance FSMs for mechanics like BWL Suppression Devices and
    /// MC pre-Majordomo rune dousing.
    fn nearby_gameobject_by_entry(&self, _entry: u32, _range: f32) -> Option<u64> {
        None
    }
    /// Invoke `GameObject::Use(Player*)` on `handle`. Returns `false` if the
    /// handle no longer resolves to a spawned GO.
    fn use_gameobject(&self, _handle: u64) -> bool {
        false
    }
    /// Convenience wrapper: true if the bot holds at least one `item_id` in
    /// backpack or carried bags (excludes bank). Default impl calls
    /// [`Self::item_count_in_bags`] so FFI mocks only need to stub the
    /// underlying count method.
    fn has_item(&self, item_id: ItemId) -> bool {
        self.item_count_in_bags(item_id) > 0
    }

    /* ── Factory: inventory mutation ─────────────────────────────────── */

    /// Destroy every equipped item plus every item in backpack + carried bags.
    /// Bank contents are left intact. Used by the factory before re-rolling gear.
    fn inventory_destroy_equipped_and_bags(&self) {}

    /// Destroy every item the bot owns (equipped, bags, and bank).
    fn inventory_destroy_all(&self) {}

    /// Return how many of `item_id` the bot holds in backpack + carried bags
    /// (excludes bank). Used by factory restock checks.
    fn item_count_in_bags(&self, _item_id: ItemId) -> u32 {
        0
    }

    /// Add `count` of `item_id` to the bot's bags. Returns the number actually
    /// added (may be less than requested if bags are full).
    fn inventory_add_item(&self, _item_id: ItemId, _count: u32) -> u32 {
        0
    }

    /// Max stack size for `item_id` (1 if the item prototype is unknown).
    fn item_max_stack_size(&self, _item_id: ItemId) -> u32 {
        1
    }

    /* ── Factory: consumable selection ────────────────────────────────── */

    /// Pick a potion item ID appropriate for `level` and spell effect
    /// (10 = `SPELL_EFFECT_HEAL`, 30 = `SPELL_EFFECT_ENERGIZE`). Returns 0 if
    /// no suitable potion exists for that level/effect.
    fn factory_pick_potion_for_level(&self, _level: u32, _effect: u32) -> ItemId {
        ItemId::NONE
    }

    /// Pick a food item ID appropriate for `level` and food category
    /// (11 = food, 59 = drink). Returns 0 if none.
    fn factory_pick_food_for_level(&self, _level: u32, _category: u32) -> ItemId {
        ItemId::NONE
    }

    /* ── RNG ─────────────────────────────────────────────────────────── */

    /// Uniform random integer in `[min, max]` (inclusive).
    fn random_u32(&self, _min: u32, _max: u32) -> u32 {
        0
    }

    /* ── Factory: progression wipe ───────────────────────────────────── */

    /// Zero out one trade skill on the bot. Called from the factory when
    /// re-rolling a bot's trade professions.
    fn bot_clear_skill(&self, _skill_id: u32) {}

    /// Reset the bot's spellbook to the class's starter spells
    /// (removes all learned spells except the defaults).
    fn bot_reset_spells(&self) {}

    /// Clear every quest from the bot's quest log and drop any
    /// completed/rewarded quest status (also deletes the DB rows).
    fn bot_reset_all_quests(&self) {}

    /* ── Factory: misc pre/post init ─────────────────────────────────── */

    /// Strip every aura (buffs and debuffs) currently on the bot.
    fn bot_remove_all_auras(&self) {}

    /// Remove all stacks of a specific spell's aura from the bot.
    fn remove_aura(&self, _spell_id: SpellId) {}

    /// Whether the bot has the given skill learned (at any rank).
    fn bot_has_skill(&self, _skill_id: u32) -> bool {
        false
    }

    /// Teach the bot a spell (`Player::learnSpell` with `dependent=false`).
    /// Used by the factory mount / spell initialization steps.
    fn bot_learn_spell(&self, _spell_id: u32) {}

    /// Remove a learned spell (`Player::removeSpell`). Used by RTSC
    /// `rtsc reset` to unlearn Aedm (spell 30758); mirrors PB2
    /// `RtscAction.cpp:33`.
    fn bot_remove_spell(&self, _spell_id: u32) {}

    /// Teach the bot its race/class starter spells — wraps
    /// `Player::learnDefaultSpells()`.
    fn bot_learn_default_spells(&self) {}

    /// Teach every class spell available at the bot's current level — wraps
    /// `Player::learnClassLevelSpells`. `include_quest_rewards=true` also
    /// folds in quest-reward spells the bot qualifies for by level.
    fn bot_learn_class_level_spells(&self, _include_quest_rewards: bool) {}

    /* ── Spell store queries ─────────────────────────────────────────── */

    /// Look up a subset of `SpellEntry` fields for `spell_id`. Returns
    /// `None` when the id is not in the server's spell store.
    fn get_spell_info(&self, _spell_id: u32) -> Option<BotSpellInfo> {
        None
    }

    /// List the bot's currently-known (non-removed, non-disabled) spell IDs.
    /// Returns an empty vec when the bot has no spells.
    fn get_bot_spells(&self) -> BotSpellList<'_> {
        OwnedList::empty()
    }

    /// Resolve a spell name (case-insensitive) to a spell ID by querying the
    /// server's spell store. Returns the highest rank the bot knows, or the
    /// highest rank in the store if the bot doesn't know any. Returns 0 on miss.
    fn resolve_spell_by_name(&self, _name: &str) -> u32 {
        0
    }

    /// Resolve an item name (case-insensitive) to an item ID by querying the
    /// server's item store. Prefers exact matches over substring matches.
    /// Returns 0 on miss.
    fn resolve_item_by_name(&self, _name: &str) -> u32 {
        0
    }

    /// Equip an item by ID in the bot's best available slot.
    /// Returns true if the item was successfully equipped.
    fn equip_item(&self, _item_id: ItemId) -> bool {
        false
    }

    /// Transfer group/raid leadership from the bot to `target_guid`.
    /// Returns true if the bot was the leader and the transfer succeeded.
    fn give_leader(&self, _target_guid: UnitHandle) -> bool {
        false
    }

    /// Resolve a player name (case-insensitive) to their `ObjectGuid`.
    /// Returns 0 when no player matches.
    fn resolve_player_by_name(&self, _name: &str) -> UnitHandle {
        0
    }

    /// Unequip the item with the given `item_id` from equipped slots, moving it to bags.
    /// Returns true if the item was successfully unequipped.
    fn unequip_item(&self, _item_id: ItemId) -> bool {
        false
    }

    /// Invite a player (by GUID) to the bot's group, creating one if needed.
    fn invite_to_group(&self, _target_guid: UnitHandle) -> bool {
        false
    }

    /// Destroy the first stack of `item_id` found in inventory.
    fn destroy_item(&self, _item_id: ItemId) -> bool {
        false
    }

    /// Share a quest with the party. `quest_id=0` means first shareable quest.
    fn share_quest(&self, _quest_id: u32) -> bool {
        false
    }

    /// Perform a text emote (/wave, /dance, etc).
    fn do_text_emote(&self, _emote_id: u32) -> bool {
        false
    }

    /* ── Bag slot management ─────────────────────────────────────────── */

    /// Number of empty equipped bag slots (0..=4).
    fn bot_empty_bag_slot_count(&self) -> u32 {
        0
    }

    /// Store `count` of `item_id` via `Player::StoreNewItemInBestSlots`,
    /// auto-equipping bags into empty bag slots. Returns `true` on success.
    fn bot_store_new_in_best_slots(&self, _item_id: ItemId, _count: u32) -> bool {
        false
    }

    /// Set reputation with `faction_id` to `value` standing points. Returns
    /// `true` when the faction exists and has a reputation list, `false`
    /// otherwise. Used by the factory reputation initialization step.
    fn bot_set_reputation(&self, _faction_id: u32, _value: i32) -> bool {
        false
    }

    /* ── Ammo management ─────────────────────────────────────────────── */

    /// Weapon `SubClass` of the item in the ranged slot, or `u32::MAX` when
    /// no ranged weapon is equipped.
    fn bot_equipped_ranged_subclass(&self) -> u32 {
        u32::MAX
    }

    /// Item id of the currently-equipped ammo (`PLAYER_AMMO_ID`), or 0.
    fn bot_current_ammo_id(&self) -> u32 {
        0
    }

    /// Wraps `sRandomItemMgr.GetAmmo(level, ammo_subclass)`. Returns 0 when
    /// no suitable ammo exists.
    fn factory_pick_ammo_for_level(&self, _level: u32, _ammo_subclass: u32) -> u32 {
        0
    }

    /// Equip `item_id` as the bot's active ammo (`Player::SetAmmo`).
    fn bot_set_ammo(&self, _item_id: u32) {}

    /* ── Skills ──────────────────────────────────────────────────────── */

    /// Current value of `skill_id` (0 when the skill is not known).
    fn bot_get_skill_value(&self, _skill_id: u32) -> u32 {
        0
    }

    /// Set (and, if necessary, learn) `skill_id` with the given value / max.
    fn bot_set_skill(&self, _skill_id: u32, _value: u32, _max: u32) {}

    /// `Player::UpdateSkillsForLevel(true)`.
    fn bot_update_skills_for_level(&self) {}

    /* ── Factory: refresh ────────────────────────────────────────────── */

    /// Bitmask of cheats currently enabled on this bot (parsed from
    /// `AiPlayerbot.BotCheats`). Bit 5 = item cheat; `PlayerbotFactory::Refresh`
    /// gates consumable top-ups on this. Returns 0 when no cheats are set.
    fn bot_cheat_mask(&self) -> u32 {
        0
    }

    /// `Player::SaveToDB()` guarded by a CharacterDatabase worker-queue check
    /// (mirrors `sRandomPlayerbotMgr.GetDatabaseDelay("CharacterDatabase") <
    /// 10ms` in `PlayerbotFactory::Refresh`). No-op when the queue is busy.
    fn bot_save_to_db_if_not_busy(&self) {}

    /* ── Factory: prepare ────────────────────────────────────────────── */

    /// `Player::ResurrectPlayer(1.0, false)` followed by
    /// `Player::SpawnCorpseBones()`. Brings a dead bot back to life in one
    /// atomic FFI call — the C++ side of `PlayerbotFactory::Prepare` always
    /// does both together.
    fn bot_resurrect_full(&self) {}

    /// `Player::CombatStop(true)` — drops target, clears threat, stops any
    /// in-progress cast. Called by `Prepare` before reshaping the bot.
    fn bot_combat_stop(&self) {}

    /// Set the bot's level to `level` and reset its current XP to 0 plus
    /// next-level XP to `sObjectMgr.GetXPForLevel(level)`. Atomic wrapper
    /// around the three C++ field writes in `PlayerbotFactory::Prepare`.
    fn bot_set_level_and_reset_xp(&self, _level: u32) {}

    /// Set or clear a bit in `PLAYER_FLAGS` via `Player::SetFlag` /
    /// `Player::RemoveFlag`. Used by `Prepare` for the helmet / cloak
    /// display flags; generalised so future factory steps can reuse it.
    fn bot_set_player_flag(&self, _flag: u32, _set: bool) {}

    /// `sPlayerbotAIConfig.disableRandomLevels` — when true, factory runs
    /// skip the level / XP reset in `Prepare` and bail out of `Randomize`.
    fn factory_config_disable_random_levels(&self) -> bool {
        false
    }

    /// `sPlayerbotAIConfig.randomBotShowHelmet` — when false, bots get the
    /// `PLAYER_FLAGS_HIDE_HELM` flag set during `Prepare`.
    fn factory_config_random_bot_show_helmet(&self) -> bool {
        true
    }

    /// `sPlayerbotAIConfig.randomBotShowCloak` — when false, bots get the
    /// `PLAYER_FLAGS_HIDE_CLOAK` flag set during `Prepare`.
    fn factory_config_random_bot_show_cloak(&self) -> bool {
        true
    }

    /* ── Factory: Randomize orchestration ────────────────────────────── */

    /// `sRandomPlayerbotMgr.IsRandomBot(bot)` — is the bot currently tracked
    /// in the random-bot account list? `PlayerbotFactory::Randomize` uses this
    /// to decide whether to run the full re-roll pipeline or just the "real
    /// player factory request" (incremental) slice.
    fn factory_is_random_bot(&self) -> bool {
        false
    }

    /// `ai->HasRealPlayerMaster()` — proxy through the PlayerbotRust accessor.
    /// Used by `Randomize` to distinguish a random bot that is claimed by a
    /// live player (do not wipe inventory / talents / auras) from a truly
    /// unclaimed random bot.
    fn factory_has_real_player_master(&self) -> bool {
        false
    }

    /// `ai->IsInRealGuild()` — same "is this bot claimed" gate as
    /// [`factory_has_real_player_master`], but checks the bot's guild instead
    /// of its master. A random bot in a guild that contains any real player
    /// is treated as claimed.
    fn factory_is_in_real_guild(&self) -> bool {
        false
    }

    /// `sPlayerbotAIConfig.minEnchantingBotLevel` — the cut-off level below
    /// which `PlayerbotFactory::Randomize` skips `LoadEnchantContainer`. The
    /// enchant container population happens C++-side when this returns true;
    /// the Rust layer only needs the scalar to decide whether to forward the
    /// trigger callback.
    fn factory_config_min_enchanting_bot_level(&self) -> u32 {
        0
    }

    /// `PlayerbotFactory::LoadEnchantContainer` — populate the per-bot
    /// enchant template container from the world DB. Called from
    /// `Randomize` after the min-enchanting-level gate passes. Delegated to
    /// C++ because the container lives on the (deleted) factory object; the
    /// Rust side has no table for it.
    fn factory_load_enchant_container(&self) {}

    /// `bot->resetTalents(true)` — wipe every learned talent. Called by
    /// `Randomize` in the non-incremental random-bot branch before learning
    /// fresh talents. The `true` arg tells CMaNGOS to refund the gold cost.
    fn bot_reset_talents(&self) {}

    /// `bot->learnQuestRewardedSpells()` — fold every quest-reward spell the
    /// bot qualifies for into its spellbook in one pass. Called by
    /// `Randomize` after rewarding the "special quest list" on a real random
    /// bot.
    fn bot_learn_quest_rewarded_spells(&self) {}

    /// `bot->GetMoney()` — current copper balance. Used by `Randomize` to
    /// top up rather than overwrite gold in the incremental branch.
    fn bot_get_money(&self) -> u32 {
        0
    }

    /// `bot->SetMoney(amount)` — set the bot's copper balance. Called by
    /// `Randomize` at the tail of the random-bot branch to hand out a small
    /// random stipend.
    fn bot_set_money(&self, _amount: u32) {}

    /// `PlayerbotFactory::InitGems` — socket gem fan-out for TBC / WotLK.
    /// Ports the whole per-slot `CMSG_SOCKET_GEMS` construction loop in one
    /// atomic callback. Classic has no sockets, so Rust only forwards this
    /// for `tbc`/`wotlk` feature builds. No-op on Classic bridges.
    fn factory_init_all_gems(&self) {}

    /// `PlayerbotFactory::EnchantEquipment` — per-slot enchant template
    /// dispatch. Ports the whole "for every equipped item, look up an
    /// enchant template and call `EnchantItem` on it" loop in one atomic
    /// callback. The C++ side is the only place that owns the enchant
    /// container, so the policy layer has nothing to iterate over itself.
    fn factory_enchant_all_equipment(&self) {}

    /* ── Factory: quests ─────────────────────────────────────────────── */

    /// Wraps `Player::SatisfyQuestClass(q,false) && q->GetMinLevel() <=
    /// bot->GetLevel() && Player::SatisfyQuestRace(q,false)` — the combined
    /// eligibility filter from `PlayerbotFactory::InitQuests`. The C++ side
    /// looks up the quest template, so the Rust caller only needs the id.
    /// Returns false when the quest is unknown.
    fn quest_is_eligible_for_bot(&self, _quest_id: u32) -> bool {
        false
    }

    /// Wraps `Player::SetQuestStatus(id, QUEST_STATUS_COMPLETE)` followed by
    /// `Player::RewardQuest(q, 0, bot, false)` — the mutation half of
    /// `PlayerbotFactory::InitQuests`. Silent no-op when the quest is unknown.
    fn bot_reward_quest_complete(&self, _quest_id: u32) {}

    /* ── Factory: arena team ─────────────────────────────────────────── */

    /// `bot->GetSession()->GetAccountId()` — the account id that owns this
    /// bot character. Used by `PlayerbotFactory::InitArenaTeam` to gate
    /// random-bot-only factory work against the live random-bot account
    /// list. Returns 0 when the session is unavailable.
    fn bot_get_account_id(&self) -> u32 {
        0
    }

    /* ── Factory: guild ──────────────────────────────────────────────── */

    /// `bot->GetGuildId()` — 0 when the bot has no guild. Used by
    /// `PlayerbotFactory::InitGuild` to skip the filter/pick pass when the
    /// bot is already a member. The guild id lives on `BotWorldSnapshot`
    /// too, but `InitGuild` runs outside a snapshot window so we route
    /// through a dedicated getter to keep the call site short.
    fn factory_bot_guild_id(&self) -> u32 {
        0
    }

    /// Read-only guild summary used by `PlayerbotFactory::InitGuild` to
    /// filter and rank the `random_bot_guilds` candidate list. Returns
    /// `None` when the guild id is unknown.
    fn factory_query_guild_summary(&self, _guild_id: u32) -> Option<GuildSummary> {
        None
    }

    /// `guild->AddMember(bot->GetObjectGuid(), rank)` — join an existing
    /// guild at `rank`. Returns false when the guild is unknown or the
    /// add failed. `PlayerbotFactory::InitGuild` calls this with a random
    /// rank between `GR_OFFICER` and `GR_INITIATE` (1..=4 in
    /// `Guild::Rank`).
    fn factory_guild_add_member(&self, _guild_id: u32, _rank: u32) -> bool {
        false
    }

    /// `guild->GetRankName(rank)` — look up the display name of a rank
    /// for log output. Returns `None` when the guild is unknown or the
    /// rank is out of range.
    fn factory_get_guild_rank_name(&self, _guild_id: u32, _rank: u32) -> Option<String> {
        None
    }

    /* ── Factory: per-bot KV store ───────────────────────────────────── */

    /// `sRandomPlayerbotMgr.GetValue(bot, key)` — read a per-bot scalar
    /// persisted in the random-bot event table. Used by
    /// `PlayerbotFactory::InitTradeSkills` to cache the two professions
    /// assigned to a bot so re-rolls keep the same pair.
    ///
    /// Returns 0 when the key is absent.
    fn factory_kv_get_u32(&self, _key: &str) -> u32 {
        0
    }

    /// `sRandomPlayerbotMgr.SetValue(bot, key, value)` — write a per-bot
    /// scalar back to the event table. The C++ manager is the sole
    /// authoritative store; the Rust `EventCache` picks up the update
    /// on its next tick.
    fn factory_kv_set_u32(&self, _key: &str, _value: u32) {}

    /// `PlayerbotFactory::InitTradeSkills` trainer-iteration loop —
    /// walk every creature template, inspect the trainer spell list,
    /// and call `bot->learnSpell` for every recipe the bot qualifies
    /// for under the "Apprentice + chosen profession or secondary"
    /// filter. Delegated to C++ because the iteration touches
    /// `sCreatureStorage` + `sSpellTemplate` + `m_trainerSpells` and
    /// would require several new FFI surfaces (plus adding
    /// `effect_trigger_spell` to `BotSpellInfo`) for zero behavioural
    /// benefit — the policy layer above is thin enough that moving
    /// just the loop across keeps the Rust port useful without
    /// bloating the vtable.
    fn factory_learn_tradeskill_recipes(&self) {}

    /* ── Item prototype queries ──────────────────────────────────────── */

    /// `ItemPrototype::Quality` (0..7). Returns 0 when the item id is unknown.
    fn item_prototype_quality(&self, _item_id: u32) -> u32 {
        0
    }

    /* ── Factory: equipment ──────────────────────────────────────────── */

    /// `bot->GetGUIDLow()` — used as the key into the itempool's
    /// per-player caches (`live_stat_weight`, `has_same_quest_rewards`).
    /// Returns 0 when the bot handle is unavailable.
    fn factory_bot_guid_low(&self) -> u32 {
        0
    }

    /// Item id currently equipped in `slot` (0..=18 per
    /// `EquipmentSlots`). Returns 0 when the slot is empty or the bot
    /// handle is unavailable. Mirrors `bot->GetItemByPos(INVENTORY_SLOT_BAG_0, slot)`
    /// followed by `GetProto()->ItemId` in `InitEquipment`.
    fn factory_bot_equipped_item_in_slot(&self, _slot: u8) -> u32 {
        0
    }

    /// Walk every equipped slot and `Player::DestroyItem` each one.
    /// Mirrors the `DestroyItemsVisitor(bot)` +
    /// `InventoryIterateItems(ITERATE_ITEMS_IN_EQUIP)` pass at the top
    /// of `PlayerbotFactory::InitEquipment` (non-incremental branch).
    fn factory_destroy_all_equipped_items(&self) {}

    /// Equip `item_id` in `slot`, optionally overwriting the existing
    /// item's random property with `random_enchant_id` (0 = no rewrite)
    /// and applying `PlayerbotFactory::EnchantItem` after a successful
    /// equip. Mirrors the atomic equip-and-enchant tail at the bottom of
    /// `InitEquipment`'s per-slot loop:
    ///
    /// ```text
    /// if (CanEquipUnseenItem(..., eDest, ...) == EQUIP_ERR_OK) {
    ///     if (oldItem) bot->DestroyItem(...);
    ///     Item* pItem = bot->EquipNewItem(eDest, newItemId, true);
    ///     if (pItem) {
    ///         if (randomEnchBestId) SetItemRandomProperties(...);
    ///         if (apply_enchants) EnchantItem(pItem);
    ///     }
    /// }
    /// ```
    ///
    /// The C++ side is responsible for destroying any existing item in
    /// the same slot before equipping the new one. Returns `true` when
    /// the equip succeeded (`pItem != nullptr`).
    fn factory_equip_new_item_in_slot(
        &self,
        _slot: u8,
        _item_id: u32,
        _random_enchant_id: u32,
        _apply_enchants: bool,
    ) -> bool {
        false
    }

    /// `Player::InitStatsForLevel(true)` + `Player::UpdateAllStats()` —
    /// the stat-refresh tail of `InitEquipment` so newly-equipped items
    /// actually contribute to the bot's stat block.
    fn factory_init_stats_for_level_and_update(&self) {}

    /// `ai->GetMaster()->GetEquipGearScore(..)` — the master's current
    /// gear score, or `None` when the bot has no master. Used by
    /// `InitEquipment` when `syncWithMaster` is set.
    fn factory_master_equip_gear_score(&self) -> Option<u32> {
        None
    }

    /// `ai->TellPlayerNoFacing(ai->GetMaster(), msg)` — broadcast a
    /// human-readable message to the bot's master. Silent no-op when
    /// the bot has no master. Used by the master-sync tail of
    /// `InitEquipment`.
    fn factory_tell_master(&self, _msg: &str) {}

    /* ── Factory: pet ────────────────────────────────────────────────── */

    /// `bot->GetPet() != nullptr` — does this bot currently have an
    /// active pet? Used by both `InitPet` (to decide whether to create
    /// one) and `InitPetSpells` (which short-circuits when there is no
    /// pet). Mirrors `PlayerbotFactory.cpp:320` / `:450`.
    fn factory_bot_has_pet(&self) -> bool {
        false
    }

    /// `bot->GetPet()->GetEntry()` — creature template id for the
    /// active pet, or `0` when the bot has no pet. Used by
    /// `InitPetSpells` to fan out to per-pet warlock tables and to
    /// decode the hunter pet family.
    fn factory_pet_entry(&self) -> u32 {
        0
    }

    /// `sObjectMgr.GetCreatureTemplate(pet_entry)->Family` — the
    /// CreatureFamily bucket (1=Wolf, 2=Cat, 3=Spider, …). Drives the
    /// hunter pet spell dispatch on vanilla. Returns `0` when the bot
    /// has no pet or the template lookup fails.
    fn factory_pet_family(&self) -> u32 {
        0
    }

    /// `bot->GetPet()->GetLevel()` — the pet's current level. Usually
    /// matches the bot (InitPet sets it that way) but we re-query
    /// rather than depending on the assumption. Returns `0` when the
    /// bot has no pet.
    fn factory_pet_level(&self) -> u32 {
        0
    }

    /// `pet->HasSpell(spell_id)` — is this spell already in the pet's
    /// spellbook? Used to avoid double-learning in `InitPetSpells`.
    fn factory_pet_has_spell(&self, _spell_id: u32) -> bool {
        false
    }

    /// Non-passive, non-removed spell IDs from the pet's current
    /// `PetSpellMap`. Used at the end of `InitPet` to mass-toggle
    /// autocast. The C++ side filters out `PETSPELL_REMOVED` entries
    /// and spells where `IsPassiveSpell` returns true, matching
    /// `PlayerbotFactory.cpp:425-435`.
    fn factory_pet_autocast_candidate_spells(&self) -> BotSpellList<'_> {
        OwnedList::empty()
    }

    /// Tameable creature ids whose `MinLevel <= bot_level`. One-shot
    /// enumeration used by `InitPet` to pick a random hunter pet;
    /// mirrors the `sCreatureStorage.LookupEntry<CreatureInfo>` walk
    /// at `PlayerbotFactory.cpp:327-345`. On WotLK the C++ side
    /// additionally gates on `CanTameExoticPets`.
    fn factory_tameable_creatures_for_bot_level(&self) -> BotSpellList<'_> {
        OwnedList::empty()
    }

    /// Atomic hunter-pet creation. Mirrors the body of the
    /// 100-iteration retry loop in `PlayerbotFactory::InitPet` minus
    /// the retry itself — the Rust caller owns the retry policy so the
    /// callback runs the whole `Pet::Create` → `AIM_Initialize` →
    /// `SavePetToDB` sequence in one shot. Returns `true` on success.
    fn factory_create_hunter_pet(&self, _creature_entry: u32) -> bool {
        false
    }

    /// Re-run the pet refresh block that lives at the tail of
    /// `PlayerbotFactory::InitPet`:
    ///
    /// ```text
    /// pet->InitStatsForLevel(bot->GetLevel());
    /// pet->SetLevel(bot->GetLevel());
    /// pet->SetLoyaltyLevel(BEST_FRIEND);        // non-WotLK only
    /// pet->SetPower(POWER_HAPPINESS, HAPPINESS_LEVEL_SIZE * 2);
    /// pet->SetHealth(pet->GetMaxHealth());
    /// pet->SetFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED);
    /// pet->AI()->SetReactState(REACT_DEFENSIVE);
    /// ```
    ///
    /// No-op when the bot has no pet.
    fn factory_pet_refresh_stats(&self) {}

    /// `pet->learnSpell(spell_id)` — add a spell to the pet's
    /// spellbook. No-op when the bot has no pet.
    fn factory_pet_learn_spell(&self, _spell_id: u32) {}

    /// `pet->ToggleAutocast(spell_id, enable)` — flip the autocast bit
    /// on a pet spell. Used by `InitPet` for the mass-on pass and by
    /// `InitPetSpells` to switch Cower off per-spell.
    fn factory_pet_toggle_autocast(&self, _spell_id: u32, _enable: bool) {}

    /// `pet->SetDeathState(JUST_DIED)` — the "force dismiss pet to fix
    /// missing flags" workaround at the end of `InitPet`. The Rust
    /// caller only invokes this when the pet is currently alive.
    fn factory_pet_force_dismiss(&self) {}

    /* ── Random item picks ───────────────────────────────────────────── */

    /// Wraps `sRandomItemMgr.GetRandomTrade(level)`. Returns 0 when no
    /// suitable trade good exists.
    fn factory_pick_trade_for_level(&self, _level: u32) -> u32 {
        0
    }

    /* ── Config list queries ─────────────────────────────────────────── */

    /// Snapshot of `sPlayerbotAIConfig.randomBotSpellIds` — the list of spell
    /// IDs handed to every bot by `InitSpecialSpells`. Returns an empty vec
    /// when the config list is empty.
    fn get_random_bot_spell_ids(&self) -> BotSpellList<'_> {
        OwnedList::empty()
    }

    /* ── Taxi nodes ──────────────────────────────────────────────────── */

    /// Overworld taxi nodes filtered for the bot's team (0=Alliance, 1=Horde).
    /// Caller gets `(node_index, map_id)` pairs; the factory picks which ones
    /// to flag on the bot via `bot_set_taxi_node`.
    fn get_overworld_taxi_nodes(&self, _team: u8) -> TaxiNodeList<'_> {
        OwnedList::empty()
    }

    /// Mark `node_index` as discovered on this bot's taxi mask.
    fn bot_set_taxi_node(&self, _node_index: u32) {}

    /// Position of the nearest taxi node (flight master) to the bot. `None`
    /// when the bot's map has no taxi network. The bot walks here, then calls
    /// [`Self::take_taxi_toward`].
    fn nearest_taxi_node_pos(&self) -> Option<BotPosition> {
        None
    }

    /// Take a flight from the flight master the bot is standing at toward the
    /// destination (nearest node to it), computing the multi-hop route.
    /// Returns false if not at a flight master, unreachable by flight, or the
    /// bot can't afford the fare.
    fn take_taxi_toward(&self, _dest_map: u32, _x: f32, _y: f32, _z: f32) -> bool {
        false
    }

    /// Cross-continent travel via boats/zeppelins toward `dest_map`. Returns a
    /// `(state, dock)` pair: state `0` = no transport route, `1` = disembarked
    /// (arrived), `2` = riding, `3` = just boarded, `4` = walk to `dock` and
    /// wait for the transport. `dock` is `Some` only for state `4`.
    fn cross_continent_travel(&self, _dest_map: u32) -> (u8, Option<BotPosition>) {
        (0, None)
    }

    /* ── Talents ─────────────────────────────────────────────────────── */

    /// All `TalentEntry` rows belonging to `spec_no` (0..2) that match the
    /// bot's class mask. Returned as owned data — the FFI malloc is freed
    /// inside the wrapper.
    fn get_class_talents(&self, _spec_no: u8) -> TalentList<'_> {
        OwnedList::empty()
    }

    /// Current `Player::GetFreeTalentPoints()`.
    fn bot_free_talent_points(&self) -> u32 {
        0
    }

    /// Wraps `Player::UpdateFreeTalentPoints(false)`. Recomputes the free
    /// talent point count after a talent spell has been learned.
    fn bot_update_free_talent_points(&self) {}

    /// Pick (or recall) the bot's talent spec tab (0..=2). See the matching
    /// doc comment on `bot_pick_spec_no` in `botffi.h` for the full policy.
    fn bot_pick_spec_no(&self, _incremental: bool) -> u32 {
        0
    }

    /// Returns the bot's dominant talent tab (0/1/2) by examining actual
    /// talent point investment. Mirrors PB2's `AiFactory::GetPlayerSpecTab`.
    fn bot_get_spec_tab(&self) -> u32 {
        0
    }

    /* ── Chat-command helpers (Wave 2) ───────────────────────────────── */

    /// Make the bot jump in place (vertical knockback).
    fn bot_jump(&self) -> bool {
        false
    }

    /// Use a hearthstone if the bot has one in its bags.
    fn bot_use_hearthstone(&self) -> bool {
        false
    }

    /// Snapshot every faction the bot has an entry for.
    fn bot_get_reputation_list(&self) -> ReputationList<'_> {
        OwnedList::empty()
    }

    /// Snapshot every skill the bot has learned (skill id + current/max).
    fn bot_get_learned_skills(&self) -> SkillList<'_> {
        OwnedList::empty()
    }

    /// Accept every quest the given NPC offers.
    fn bot_quest_accept_from(&self, _npc: UnitHandle) -> bool {
        false
    }

    /// Abandon an in-progress quest (removes it from the log).
    fn bot_quest_abandon(&self, _quest_id: u32) -> bool {
        false
    }

    /* ── Chat-command helpers (Wave 3: mail + guild) ─────────────────── */

    /// Mailbox summary — totals only, no per-mail details.
    fn bot_mail_summary(&self) -> BotMailSummary {
        BotMailSummary {
            total_mails: 0,
            mails_with_money: 0,
            mails_with_items: 0,
            total_money: 0,
        }
    }

    /// Take all money and items from every mail in the inbox. Requires the
    /// bot to be next to a mailbox `GameObject`.
    fn bot_mail_take_all(&self) -> bool {
        false
    }

    /// Leave the bot's current guild. Returns false if not in a guild or
    /// if the bot is the guild master.
    fn bot_guild_leave(&self) -> bool {
        false
    }

    /* ── RTSC / file I/O helpers ─────────────────────────────────────── */

    /// Summon a temporary marker creature (used for RTSC waypoints).
    fn bot_summon_marker_creature(
        &self,
        _entry: u32,
        _x: f32,
        _y: f32,
        _z: f32,
        _o: f32,
        _despawn_ms: u32,
        _scale: f32,
    ) {
    }

    /// Write a string body to a bot-data file under the server logs dir.
    fn bot_write_log_file(&self, _name: &str, _body: &str) -> bool {
        false
    }

    /// Append a line to a bot-data file (opens in append mode, not truncate).
    fn bot_append_log_file(&self, _name: &str, _line: &str) -> bool {
        false
    }

    /// Read the contents of a bot-data file written via `bot_write_log_file`.
    fn bot_read_log_file(&self, _name: &str) -> Option<String> {
        None
    }

    /* ── Loot rolling ───────────────────────────────────────────────── */

    /// Number of pending loot rolls the bot hasn't voted on.
    fn get_pending_roll_count(&self) -> u32 {
        0
    }

    /// Auto-roll on the next pending item (need/greed/pass based on item value).
    fn auto_loot_roll(&self) -> bool {
        false
    }

    /// Cast a specific roll vote on the next pending item.
    /// 0 = pass, 1 = need, 2 = greed.
    fn cast_loot_roll(&self, _vote: u8) -> bool {
        false
    }

    /* ── Travel destination queries ─────────────────────────────────── */

    /// Find nearby travel destinations matching `purpose_flags`.
    /// Returns up to `max_results` sorted by distance.
    fn find_travel_dests(
        &self,
        _purpose_flags: u32,
        _max_range: f32,
        _max_results: u32,
    ) -> TravelDestList<'_> {
        OwnedList::empty()
    }

    /* ── World buffs ────────────────────────────────────────────────── */

    /// Directly apply aura `spell_id` to the bot (bypasses normal casting).
    fn add_aura(&self, _spell_id: u32) -> bool {
        false
    }

    /// Get the list of world buff spell IDs the bot is missing per config
    /// (`AiPlayerbot.WorldBuff.*` with faction/class/spec/level filtering).
    /// Backed by a fixed-size stack buffer in the impl, so this stays as a
    /// plain `Vec<u32>` rather than an FFI-allocated `OwnedList`.
    fn get_needed_world_buffs(&self) -> Vec<u32> {
        vec![]
    }

    /* ── Heal interrupt ─────────────────────────────────────────────── */

    /// Interrupt the bot's own current cast. Returns true if a cast was cancelled.
    fn interrupt_own_cast(&self) -> bool {
        false
    }

    /* ── NPC interaction (gossip) ────────────────────────────────────── */

    /// Gossip-hello with a nearby NPC matching `entry`.
    fn gossip_hello(&self, _npc_entry: u32) -> bool {
        false
    }

    /// Buy `qty` of `item_id` from a nearby vendor.
    fn buy_from_vendor(&self, _item_id: u32, _qty: u32) -> bool {
        false
    }

    /* ── Mail ────────────────────────────────────────────────────────── */

    /// Send an item from the bot's bags to the master.
    fn mail_item_to_master(&self) -> bool {
        false
    }

    /* ── Bank ────────────────────────────────────────────────────────── */

    /// Deposit excess items into the bank.
    fn bank_deposit(&self) -> bool {
        false
    }

    /// Withdraw useful items from the bank.
    fn bank_withdraw(&self) -> bool {
        false
    }

    /* ── EngBags: inventory enumeration ────────────────────────────── */

    /// Return the bot's bag contents (backpack + equipped bags).
    fn bot_get_inventory(&self) -> InventoryList<'_> {
        OwnedList::empty()
    }

    /// Return the bot's equipped items.
    fn bot_get_equipped(&self) -> InventoryList<'_> {
        OwnedList::empty()
    }

    /// Return items in the bot's bank slots. Requires banker proximity.
    fn bot_get_bank_items(&self) -> InventoryList<'_> {
        OwnedList::empty()
    }

    /// Return items attached to mails in the bot's inbox.
    fn bot_get_mail_items(&self) -> InventoryList<'_> {
        OwnedList::empty()
    }

    /// Sell a specific item by ID (adds sell price to gold, destroys item).
    fn sell_item(&self, _item_id: ItemId) -> bool {
        false
    }

    /// Deposit a specific item (by ID) from bags into bank.
    fn bank_deposit_item(&self, _item_id: ItemId) -> bool {
        false
    }

    /// Withdraw a specific item (by ID) from bank into bags.
    fn bank_withdraw_item(&self, _item_id: ItemId) -> bool {
        false
    }

    /// Take items + money from a specific mail (1-based index).
    fn bot_mail_take_index(&self, _mail_index: u32) -> bool {
        false
    }

    /// Send a specific item (by ID) from bags to the master via mail.
    fn send_mail_item(&self, _item_id: ItemId) -> bool {
        false
    }

    /// Add an item to the bot's trade window. count=0 means 1 stack.
    fn trade_add_item(&self, _item_id: ItemId, _count: u32) -> bool {
        false
    }

    /// Look up the item ID created by a tradeskill spell.
    fn get_spell_craft_item(&self, _spell_id: u32) -> u32 {
        0
    }

    /// Look up item name and quality by ID. Returns `(name, quality)` or None.
    fn get_item_info(&self, _item_id: u32) -> Option<(String, u8)> {
        None
    }

    /* ── Auction house ───────────────────────────────────────────────── */

    /// Post items on the auction house.
    fn ah_post(&self) -> bool {
        false
    }

    /// Bid on auction house listings.
    fn ah_bid(&self) -> bool {
        false
    }

    /* ── Fishing ─────────────────────────────────────────────────────── */

    /// Start fishing (equip pole + cast).
    fn start_fishing(&self) -> bool {
        false
    }

    /* ── BG/Arena ────────────────────────────────────────────────────── */

    /// Queue the bot for a random battleground.
    fn queue_bg(&self) -> bool {
        false
    }

    /// Accept a pending BG invitation.
    fn accept_bg_invite(&self) -> bool {
        false
    }

    /// Get the position of a BG objective.
    /// `objective_type`: 0=defend, 1=assault, 2=flag, `3=return_flag`.
    fn get_bg_objective_pos(&self, _objective_type: u8) -> BotPosition {
        BotPosition {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            o: 0.0,
            map_id: 0,
        }
    }

    /* ── LFG ─────────────────────────────────────────────────────────── */

    /// Join the LFG queue (`WotLK` only).
    fn lfg_join(&self) -> bool {
        false
    }

    /// Accept a pending LFG proposal (`WotLK` only).
    fn lfg_accept(&self) -> bool {
        false
    }

    /* ── Dungeon awareness ───────────────────────────────────────────── */

    /// Get the tank's current position for stay-near-tank logic.
    fn get_tank_position(&self) -> BotPosition {
        BotPosition {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            o: 0.0,
            map_id: 0,
        }
    }

    /// Check if the given target has an active CC.
    fn is_unit_cc(&self, _target: UnitHandle) -> bool {
        false
    }

    /* ── Debug ───────────────────────────────────────────────────────── */

    /// Dump debug state. kind: 0=full, 1=strategies, 2=blackboard.
    fn debug_dump_state(&self, _kind: u8) -> bool {
        false
    }
}
