//! Bot factory — owns the bot-creation / re-roll logic that was previously in
//! the C++ `PlayerbotFactory`. Factory code does not run every tick; it runs
//! once when a bot is generated, and occasionally thereafter (gear re-rolls,
//! consumable top-ups, spec changes).
//!
//! Each submodule covers one concern of the old `PlayerbotFactory`:
//!
//!   * `inventory`   — clear / restock items, bags.
//!   * `consumables` — potions, food, reagents, totems.
//!   * `progression` — clear trade skills / spellbook / quest log.
//!   * `misc`        — cancel auras, hand out trade-skill tools.
//!
//! Submodules are pure policy: they take `&dyn World` and call methods
//! on it. They do not touch `CMaNGOS` directly — the FFI layer handles that.

pub mod ammo;
pub mod arena_team;
pub mod available_spells;
pub mod consumables;
pub mod equipment;
pub mod guild;
pub mod init_trade_skills;
pub mod inventory;
pub mod inventory_trade;
pub mod misc;
pub mod mounts;
pub mod pet;
pub mod progression;
pub mod quests;
pub mod randomize;
pub mod reputations;
pub mod skills;
pub mod special_spells;
pub mod talents;
pub mod taxi_nodes;
pub mod transaction;

pub use transaction::FactoryTransaction;

#[cfg(test)]
pub(crate) use transaction::with_tx;

/// What slice of the inventory a factory clear should wipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearScope {
    /// Equipped slots + backpack + carried bags. Bank is left intact.
    EquippedAndBags,
    /// Everything the bot owns — equipped, bags, and bank.
    All,
}

impl ClearScope {
    /// Decode the scalar mode passed over the FFI.
    pub fn from_mode(mode: u8) -> Self {
        match mode {
            0 => ClearScope::EquippedAndBags,
            _ => ClearScope::All,
        }
    }
}

/// Clear the bot's inventory to the requested scope.
pub fn clear_inventory(tx: &mut FactoryTransaction<'_>, scope: ClearScope) {
    inventory::clear(tx, scope);
}

/// What kind of consumable restock to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumableKind {
    Potions,
    Food,
    Reagents,
    /// Class-specific weapon consumables (caster oils, melee stones, rogue
    /// poisons). Ports `PlayerbotFactory::AddConsumables`.
    AdditionalConsumables,
}

impl ConsumableKind {
    /// Decode the scalar kind passed over the FFI.
    pub fn from_kind(kind: u8) -> Option<Self> {
        match kind {
            0 => Some(Self::Potions),
            1 => Some(Self::Food),
            2 => Some(Self::Reagents),
            3 => Some(Self::AdditionalConsumables),
            _ => None,
        }
    }
}

/// Bit 5 of the `BotCheats` config mask. Gates consumable restock in
/// `refresh` — mirrors `BotCheatMask::item` in `cpp_wrapper/BotConfig.h`.
pub const BOT_CHEAT_ITEM: u32 = 1 << 5;

/// `PLAYER_FLAGS_HIDE_HELM` — matches Player.h in the Karatefylla fork.
pub const PLAYER_FLAGS_HIDE_HELM: u32 = 0x0000_0400;

/// `PLAYER_FLAGS_HIDE_CLOAK` — matches Player.h in the Karatefylla fork.
pub const PLAYER_FLAGS_HIDE_CLOAK: u32 = 0x0000_0800;

/// Port of `PlayerbotFactory::Prepare` — the "reset the chassis" pass that
/// runs at the top of `Randomize` before any spellbook / gear / talent work.
///
/// The full C++ body is:
///
/// ```text
/// if (!bot->IsAlive()) { ResurrectPlayer(1.0, false); SpawnCorpseBones(); }
/// bot->CombatStop(true);
/// if (!sPlayerbotAIConfig.disableRandomLevels) {
///     bot->SetLevel(level);
///     bot->SetUInt32Value(PLAYER_XP, 0);
///     bot->SetUInt32Value(PLAYER_NEXT_LEVEL_XP, GetXPForLevel(level));
/// }
/// if (!sPlayerbotAIConfig.randomBotShowHelmet) SetFlag(PLAYER_FLAGS, HIDE_HELM);
/// if (!sPlayerbotAIConfig.randomBotShowCloak)  SetFlag(PLAYER_FLAGS, HIDE_CLOAK);
/// ```
///
/// Level comes from the `PlayerbotFactory` ctor in C++ (caller-supplied, not
/// read from the bot) so we take it as a parameter here. `bot_resurrect_full`
/// handles the `IsAlive` gate on the C++ side so we always call it.
pub fn prepare(tx: &mut FactoryTransaction<'_>, level: u32) {
    tx.bot_resurrect_full();
    tx.bot_combat_stop();

    if !tx.factory_config_disable_random_levels() {
        tx.bot_set_level_and_reset_xp(level);
    }

    if !tx.factory_config_random_bot_show_helmet() {
        tx.bot_set_player_flag(PLAYER_FLAGS_HIDE_HELM, true);
    }

    if !tx.factory_config_random_bot_show_cloak() {
        tx.bot_set_player_flag(PLAYER_FLAGS_HIDE_CLOAK, true);
    }
}

/// Port of `PlayerbotFactory::InitQuests` — reward every quest in `quest_ids`
/// that the bot is eligible for. See `factory::quests::init_quests` for the
/// full body; this is a thin re-export so the exports-level FFI stays
/// symmetric with the other `factory::*` entry points.
pub fn init_quests(tx: &mut FactoryTransaction<'_>, quest_ids: &[u32]) {
    quests::init_quests(tx, quest_ids);
}

/// Top-level port of `PlayerbotFactory::Randomize` — chains every factory
/// step into a single "re-roll this bot from scratch" pass. See
/// [`factory::randomize::randomize`] for the full body. This is a thin
/// re-export so the exports-level FFI entry point matches the naming of
/// the other `factory::*` forwarders.
pub fn randomize(
    tx: &mut FactoryTransaction<'_>,
    level: u32,
    incremental: bool,
    sync_with_master: bool,
    item_quality: u32,
) {
    randomize::randomize(tx, level, incremental, sync_with_master, item_quality);
}

/// Port of `PlayerbotFactory::InitArenaTeam` — gate the random arena-team
/// creation pass on the bot's owning account and the current tracked team
/// count. See `factory::arena_team::init_arena_team` for the full body and
/// the Classic stub.
pub fn init_arena_team(tx: &mut FactoryTransaction<'_>) {
    arena_team::init_arena_team(tx);
}

/// Port of `PlayerbotFactory::InitGuild` — top up the guild tabard for
/// already-guilded bots, otherwise filter the random-bot guild pool by
/// team, pick a candidate, and add the bot at a random rank. See
/// `factory::guild::init_guild` for the full body.
pub fn init_guild(tx: &mut FactoryTransaction<'_>) {
    guild::init_guild(tx);
}

/// Flag-bit layout for `init_equipment`. Mirrors the four bool
/// parameters on `PlayerbotFactory::InitEquipment(incremental,
/// syncWithMaster, progressive, partialUpgrade)`.
pub mod equipment_flags {
    pub const INCREMENTAL: u32 = 1 << 0;
    pub const SYNC_WITH_MASTER: u32 = 1 << 1;
    pub const PROGRESSIVE: u32 = 1 << 2;
    pub const PARTIAL_UPGRADE: u32 = 1 << 3;
}

/// Port of `PlayerbotFactory::InitEquipment` — re-roll the bot's
/// equipped gear from the itempool cache. `flags` packs the four boolean
/// parameters using [`equipment_flags`]; `item_quality` corresponds to
/// PB2's `itemQuality` local (set by `.py` commands like `equip rare`)
/// and pins the quality tier when non-zero.
pub fn init_equipment(tx: &mut FactoryTransaction<'_>, flags: u32, item_quality: u32) {
    let incremental = flags & equipment_flags::INCREMENTAL != 0;
    let sync_with_master = flags & equipment_flags::SYNC_WITH_MASTER != 0;
    let progressive = flags & equipment_flags::PROGRESSIVE != 0;
    let partial_upgrade = flags & equipment_flags::PARTIAL_UPGRADE != 0;
    equipment::init_equipment(
        tx,
        incremental,
        sync_with_master,
        progressive,
        partial_upgrade,
        item_quality,
    );
}

/// Port of `PlayerbotFactory::InitPet` — hunter-only pet creation +
/// refresh + mass-autocast + force-dismiss pass. See
/// [`factory::pet::init_pet`] for the full body. Every other class is
/// a no-op.
pub fn init_pet(tx: &mut FactoryTransaction<'_>) {
    pet::init_pet(tx);
}

/// Port of `PlayerbotFactory::InitPetSpells` — per-class / per-
/// expansion pet spell learning. Hunter + warlock only. See
/// [`factory::pet::init_pet_spells`] for the full body.
pub fn init_pet_spells(tx: &mut FactoryTransaction<'_>) {
    pet::init_pet_spells(tx);
}

/// Port of `PlayerbotFactory::InitAllSkills` — runs `InitSkills` (weapons /
/// armor / riding) followed by `InitTradeSkills` (professions + the three
/// universal secondaries + the TBC+ armorsmith chain + the opaque trainer
/// loop). Takes class + level from a single snapshot so the policy layer
/// does not re-read the FFI for every call. See
/// `factory::init_trade_skills::init_all_skills` for the body.
pub fn init_all_skills(tx: &mut FactoryTransaction<'_>) {
    let snap = tx.get_snapshot();
    init_trade_skills::init_all_skills(
        tx,
        snap.self_.class_id,
        u32::from(snap.self_.level),
    );
}

/// Port of `PlayerbotFactory::InitTradeSkills` — the tradeskill-only half
/// of `InitAllSkills`. Kept as a standalone entry point for callers that
/// only want profession assignment + the trainer loop without touching
/// the weapon/armor/riding pass. See
/// `factory::init_trade_skills::init_trade_skills` for the body.
pub fn init_trade_skills_only(tx: &mut FactoryTransaction<'_>) {
    let snap = tx.get_snapshot();
    init_trade_skills::init_trade_skills(
        tx,
        snap.self_.class_id,
        u32::from(snap.self_.level),
    );
}

/// Port of `PlayerbotFactory::Refresh` — a "top up consumables" pass that is
/// called when a bot is re-prepared, not when it is first created.
///
/// The full C++ body is:
///
/// ```text
/// if (!ai->HasCheat(BotCheatMask::item)) return;
/// InitAmmo(); InitFood(); InitPotions(); InitReagents(); AddConsumables();
/// if (db_delay < 10ms) bot->SaveToDB();
/// ```
///
/// Every init step here has already been ported — the only new FFI surface
/// is `bot_cheat_mask` (the gate) and `bot_save_to_db_if_not_busy` (the
/// tail). Called from C++ `PlayerbotFactory::Refresh` via the Rust bridge.
pub fn refresh(tx: &mut FactoryTransaction<'_>) {
    // Gate: the item cheat must be enabled, otherwise the factory never
    // hands out free consumables on a refresh pass.
    if (tx.bot_cheat_mask() & BOT_CHEAT_ITEM) == 0 {
        return;
    }

    let snap = tx.get_snapshot();
    let level = u32::from(snap.self_.level);
    let class = snap.self_.class_id;
    let has_mana = snap.self_.power_type == 0; // POWER_MANA = 0

    ammo::init_ammo(tx, class, level);
    consumables::init_food(tx, level, has_mana);
    consumables::init_potions(tx, level, has_mana);
    consumables::init_reagents(tx, class, level);
    consumables::init_additional_consumables(tx, class, level);

    // Tail: save to DB only if the character database worker queue isn't
    // already backed up (PB2's GetDatabaseDelay < 10ms guard).
    tx.bot_save_to_db_if_not_busy();
}

/// Initialize consumables on a freshly-created or re-rolled bot.
///
/// Pulls bot level, class, and mana-user status from a single snapshot so the
/// policy functions do not round-trip the FFI more than necessary.
pub fn init_consumables(tx: &mut FactoryTransaction<'_>, kind: ConsumableKind) {
    let snap = tx.get_snapshot();
    let level = u32::from(snap.self_.level);
    let class = snap.self_.class_id;
    let has_mana = snap.self_.power_type == 0; // POWER_MANA = 0

    match kind {
        ConsumableKind::Potions => {
            consumables::init_potions(tx, level, has_mana);
        }
        ConsumableKind::Food => {
            consumables::init_food(tx, level, has_mana);
        }
        ConsumableKind::Reagents => {
            consumables::init_reagents(tx, class, level);
        }
        ConsumableKind::AdditionalConsumables => {
            consumables::init_additional_consumables(tx, class, level);
        }
    }
}

/// Which slice of a bot's progression to wipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressionKind {
    /// Trade professions (alchemy, mining, etc.).
    TradeSkills,
    /// Spellbook (reset to class starter spells).
    Spells,
    /// All active and completed quests.
    Quests,
}

impl ProgressionKind {
    /// Decode the scalar kind passed over the FFI.
    pub fn from_kind(kind: u8) -> Option<Self> {
        match kind {
            0 => Some(Self::TradeSkills),
            1 => Some(Self::Spells),
            2 => Some(Self::Quests),
            _ => None,
        }
    }
}

/// Wipe one slice of the bot's progression. Used by the factory before
/// re-rolling a character.
pub fn reset_progression(tx: &mut FactoryTransaction<'_>, kind: ProgressionKind) {
    match kind {
        ProgressionKind::TradeSkills => progression::clear_trade_skills(tx),
        ProgressionKind::Spells => progression::clear_spells(tx),
        ProgressionKind::Quests => progression::reset_all_quests(tx),
    }
}

/// Miscellaneous factory steps (pre-init cleanup, starter kits) that are
/// too small to warrant their own dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiscKind {
    /// Strip every aura from the bot (pre-init cleanup).
    CancelAuras,
    /// Give the bot the mandatory tool for each trade skill it knows.
    InitSkillToolKit,
    /// Teach the bot its race- and level-appropriate mount spells.
    InitMounts,
    /// Equip a starter bag into each empty bag slot.
    InitBags,
    /// Grant honored standing with level- and team-appropriate factions.
    InitReputations,
    /// Top up ranged-weapon ammo (arrows/bullets/thrown) for warrior/rogue/hunter.
    InitAmmo,
    /// Stock the bot with one random trade good appropriate for its level.
    InitInventoryTrade,
    /// Initialize armor/weapon/riding skill proficiencies for the bot.
    InitSkills,
    /// Teach config-listed "special" spells (e.g. Cold Weather Flying).
    InitSpecialSpells,
    /// Flag level-appropriate overworld taxi nodes on the bot.
    InitTaxiNodes,
    /// Teach the bot its default + class-level spellbook plus the hard-coded
    /// paladin/mage/warlock/classic level-60 top-ups.
    InitAvailableSpells,
    /// Zero every trade skill currently at value 1 ("learned but never
    /// trained"). Ports `PlayerbotFactory::UpdateTradeSkills`.
    UpdateTradeSkills,
}

impl MiscKind {
    /// Decode the scalar kind passed over the FFI.
    pub fn from_kind(kind: u8) -> Option<Self> {
        match kind {
            0 => Some(Self::CancelAuras),
            1 => Some(Self::InitSkillToolKit),
            2 => Some(Self::InitMounts),
            3 => Some(Self::InitBags),
            4 => Some(Self::InitReputations),
            5 => Some(Self::InitAmmo),
            6 => Some(Self::InitInventoryTrade),
            7 => Some(Self::InitSkills),
            8 => Some(Self::InitSpecialSpells),
            9 => Some(Self::InitTaxiNodes),
            10 => Some(Self::InitAvailableSpells),
            11 => Some(Self::UpdateTradeSkills),
            _ => None,
        }
    }
}

/// Run a miscellaneous factory step.
pub fn run_misc(tx: &mut FactoryTransaction<'_>, kind: MiscKind) {
    match kind {
        MiscKind::CancelAuras => misc::cancel_auras(tx),
        MiscKind::InitSkillToolKit => misc::init_skill_tool_kit(tx),
        MiscKind::InitMounts => {
            let snap = tx.get_snapshot();
            mounts::init_mounts(
                tx,
                u32::from(snap.self_.level),
                snap.self_.race_id,
                snap.self_.team,
            );
        }
        MiscKind::InitBags => misc::init_bags(tx),
        MiscKind::InitReputations => {
            let snap = tx.get_snapshot();
            reputations::init_reputations(tx, u32::from(snap.self_.level), snap.self_.team);
        }
        MiscKind::InitAmmo => {
            let snap = tx.get_snapshot();
            ammo::init_ammo(tx, snap.self_.class_id, u32::from(snap.self_.level));
        }
        MiscKind::InitInventoryTrade => {
            let snap = tx.get_snapshot();
            inventory_trade::init_inventory_trade(tx, u32::from(snap.self_.level));
        }
        MiscKind::InitSkills => {
            let snap = tx.get_snapshot();
            skills::init_skills(tx, snap.self_.class_id, u32::from(snap.self_.level));
        }
        MiscKind::InitSpecialSpells => {
            special_spells::init_special_spells(tx);
        }
        MiscKind::InitTaxiNodes => {
            let snap = tx.get_snapshot();
            taxi_nodes::init_taxi_nodes(
                tx,
                u32::from(snap.self_.level),
                snap.self_.team,
                snap.self_.pos.map_id,
            );
        }
        MiscKind::InitAvailableSpells => {
            let snap = tx.get_snapshot();
            available_spells::init_available_spells(
                tx,
                snap.self_.class_id,
                u32::from(snap.self_.level),
            );
        }
        MiscKind::UpdateTradeSkills => {
            progression::update_trade_skills(tx);
        }
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    use cmangos::{BotUnitSnapshot, ItemId, MockEvent, MockWorld};

    /// Build a `MockWorld` wired up for refresh: a mid-level bot with every
    /// factory picker returning a usable item id so any branch that reaches
    /// it produces an observable effect.
    fn make_world(class_id: u8, level: u8, cheats: u32) -> MockWorld {
        let self_snap = BotUnitSnapshot {
            class_id,
            level,
            power_type: 0, // POWER_MANA
            ..Default::default()
        };

        MockWorld::builder()
            .self_snap(self_snap)
            .cheat_mask(cheats)
            // Potion / food picks are level-indexed; feed enough rows so the
            // consumable branches actually hand out items.
            .potion_pick(u32::from(level), 1, ItemId(3928))
            .potion_pick(u32::from(level), 2, ItemId(3928))
            .food_pick(u32::from(level), 11, ItemId(3927))
            .food_pick(u32::from(level), 59, ItemId(3772))
            .build()
    }

    #[test]
    fn refresh_without_item_cheat_is_noop() {
        // No cheat bits set → early return, no inventory changes, no save.
        let mut world = make_world(1, 40, 0);
        with_tx(&mut world, refresh);

        let events = world.events();
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MockEvent::SaveToDbIfNotBusy)),
            "refresh should not save to DB when item cheat is disabled"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MockEvent::InventoryAddItem { .. })),
            "refresh should not add any items when item cheat is disabled"
        );
    }

    #[test]
    fn refresh_with_other_cheats_but_not_item_is_noop() {
        // A non-item cheat bit (taxi = 1<<0) still fails the item gate.
        let mut world = make_world(1, 40, 1);
        with_tx(&mut world, refresh);

        assert!(
            !world
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::SaveToDbIfNotBusy)),
            "refresh should only honour BOT_CHEAT_ITEM"
        );
    }

    #[test]
    fn refresh_with_item_cheat_saves_to_db() {
        // Warrior level 40: has no reagents but does have potions/food/etc,
        // so the tail always runs.
        let mut world = make_world(1, 40, BOT_CHEAT_ITEM);
        with_tx(&mut world, refresh);

        assert!(
            world
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::SaveToDbIfNotBusy)),
            "refresh should save to DB once every init step has run"
        );
    }

    #[test]
    fn refresh_with_item_cheat_tops_up_food_and_potions() {
        // Mage level 40 — mana user, so both potion and food branches run
        // and produce InventoryAddItem events.
        let mut world = make_world(8, 40, BOT_CHEAT_ITEM);
        with_tx(&mut world, refresh);

        let add_events: Vec<_> = world
            .events()
            .into_iter()
            .filter(|e| matches!(e, MockEvent::InventoryAddItem { .. }))
            .collect();

        assert!(
            !add_events.is_empty(),
            "refresh should add at least one consumable when item cheat is set"
        );
    }

    #[test]
    fn refresh_honours_exact_item_bit() {
        // BOT_CHEAT_ITEM should be bit 5, matching `BotCheatMask::item` in
        // cpp_wrapper/BotConfig.h. Guard against accidental re-numbering.
        assert_eq!(BOT_CHEAT_ITEM, 1 << 5);
    }
}

#[cfg(test)]
mod prepare_tests {
    use super::*;
    use cmangos::{MockEvent, MockWorld};

    /// Build a `MockWorld` with the three prepare-relevant config flags set
    /// to whatever defaults the test wants. All other state stays empty.
    fn make_world(disable_levels: bool, show_helm: bool, show_cloak: bool) -> MockWorld {
        MockWorld::builder()
            .disable_random_levels(disable_levels)
            .random_bot_show_helmet(show_helm)
            .random_bot_show_cloak(show_cloak)
            .build()
    }

    #[test]
    fn prepare_always_resurrects_and_stops_combat() {
        // The C++ wrapper owns the `IsAlive` gate, so Rust always calls
        // `bot_resurrect_full` — the bridge decides whether to actually run
        // ResurrectPlayer. CombatStop is unconditional.
        let mut world = make_world(false, true, true);
        with_tx(&mut world, |tx| prepare(tx, 40));

        let events = world.events();
        assert!(
            events.iter().any(|e| matches!(e, MockEvent::ResurrectFull)),
            "prepare should call bot_resurrect_full"
        );
        assert!(
            events.iter().any(|e| matches!(e, MockEvent::CombatStop)),
            "prepare should call bot_combat_stop"
        );
    }

    #[test]
    fn prepare_sets_level_when_random_levels_enabled() {
        let mut world = make_world(false, true, true);
        with_tx(&mut world, |tx| prepare(tx, 42));

        let got = world
            .events()
            .into_iter()
            .find_map(|e| match e {
                MockEvent::SetLevelAndResetXp(lvl) => Some(lvl),
                _ => None,
            });
        assert_eq!(
            got,
            Some(42),
            "prepare should set level + reset XP via the atomic FFI call"
        );
    }

    #[test]
    fn prepare_skips_level_when_random_levels_disabled() {
        let mut world = make_world(true, true, true);
        with_tx(&mut world, |tx| prepare(tx, 42));

        assert!(
            !world
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::SetLevelAndResetXp(_))),
            "prepare should skip level/XP reset when disableRandomLevels is true"
        );
    }

    #[test]
    fn prepare_sets_helmet_flag_when_show_helmet_false() {
        let mut world = make_world(false, false, true);
        with_tx(&mut world, |tx| prepare(tx, 40));

        assert!(
            world.events().iter().any(|e| matches!(
                e,
                MockEvent::SetPlayerFlag { flag, set }
                    if *flag == PLAYER_FLAGS_HIDE_HELM && *set
            )),
            "prepare should set PLAYER_FLAGS_HIDE_HELM when randomBotShowHelmet=false"
        );
        assert!(
            !world.events().iter().any(|e| matches!(
                e,
                MockEvent::SetPlayerFlag { flag, .. }
                    if *flag == PLAYER_FLAGS_HIDE_CLOAK
            )),
            "prepare should not touch the cloak flag when randomBotShowCloak=true"
        );
    }

    #[test]
    fn prepare_sets_cloak_flag_when_show_cloak_false() {
        let mut world = make_world(false, true, false);
        with_tx(&mut world, |tx| prepare(tx, 40));

        assert!(
            world.events().iter().any(|e| matches!(
                e,
                MockEvent::SetPlayerFlag { flag, set }
                    if *flag == PLAYER_FLAGS_HIDE_CLOAK && *set
            )),
            "prepare should set PLAYER_FLAGS_HIDE_CLOAK when randomBotShowCloak=false"
        );
        assert!(
            !world.events().iter().any(|e| matches!(
                e,
                MockEvent::SetPlayerFlag { flag, .. }
                    if *flag == PLAYER_FLAGS_HIDE_HELM
            )),
            "prepare should not touch the helmet flag when randomBotShowHelmet=true"
        );
    }

    #[test]
    fn prepare_skips_both_flags_when_both_show_true() {
        let mut world = make_world(false, true, true);
        with_tx(&mut world, |tx| prepare(tx, 40));

        assert!(
            !world
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::SetPlayerFlag { .. })),
            "prepare should not set any player flags when helmet/cloak are both visible"
        );
    }

    #[test]
    fn prepare_honours_exact_player_flag_bits() {
        // Guard against accidental re-numbering of the PLAYER_FLAGS_HIDE_*
        // constants. Values must match Player.h in the Karatefylla fork:
        //   PLAYER_FLAGS_HIDE_HELM  = 0x00000400
        //   PLAYER_FLAGS_HIDE_CLOAK = 0x00000800
        assert_eq!(PLAYER_FLAGS_HIDE_HELM, 0x0000_0400);
        assert_eq!(PLAYER_FLAGS_HIDE_CLOAK, 0x0000_0800);
    }
}
