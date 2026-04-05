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
//! Submodules are pure policy: they take `&dyn BotInterface` and call methods
//! on it. They do not touch `CMaNGOS` directly — the FFI layer handles that.

pub mod ammo;
pub mod available_spells;
pub mod consumables;
pub mod inventory;
pub mod inventory_trade;
pub mod misc;
pub mod mounts;
pub mod progression;
pub mod reputations;
pub mod skills;
pub mod special_spells;
pub mod talents;
pub mod taxi_nodes;

use crate::ffi::interface::BotInterface;

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
pub fn clear_inventory(iface: &dyn BotInterface, scope: ClearScope) {
    inventory::clear(iface, scope);
}

/// What kind of consumable restock to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumableKind {
    Potions,
    Food,
    Reagents,
}

impl ConsumableKind {
    /// Decode the scalar kind passed over the FFI.
    pub fn from_kind(kind: u8) -> Option<Self> {
        match kind {
            0 => Some(Self::Potions),
            1 => Some(Self::Food),
            2 => Some(Self::Reagents),
            _ => None,
        }
    }
}

/// Initialize consumables on a freshly-created or re-rolled bot.
///
/// Pulls bot level, class, and mana-user status from a single snapshot so the
/// policy functions do not round-trip the FFI more than necessary.
pub fn init_consumables(iface: &dyn BotInterface, kind: ConsumableKind) {
    let snap = iface.get_snapshot();
    let level = u32::from(snap.self_.level);
    let class = snap.self_.class_id;
    let has_mana = snap.self_.power_type == 0; // POWER_MANA = 0

    match kind {
        ConsumableKind::Potions => {
            consumables::init_potions(iface, level, has_mana);
        }
        ConsumableKind::Food => {
            consumables::init_food(iface, level, has_mana);
        }
        ConsumableKind::Reagents => {
            consumables::init_reagents(iface, class, level);
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
pub fn reset_progression(iface: &dyn BotInterface, kind: ProgressionKind) {
    match kind {
        ProgressionKind::TradeSkills => progression::clear_trade_skills(iface),
        ProgressionKind::Spells => progression::clear_spells(iface),
        ProgressionKind::Quests => progression::reset_all_quests(iface),
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
            _ => None,
        }
    }
}

/// Run a miscellaneous factory step.
pub fn run_misc(iface: &dyn BotInterface, kind: MiscKind) {
    match kind {
        MiscKind::CancelAuras => misc::cancel_auras(iface),
        MiscKind::InitSkillToolKit => misc::init_skill_tool_kit(iface),
        MiscKind::InitMounts => {
            let snap = iface.get_snapshot();
            mounts::init_mounts(
                iface,
                u32::from(snap.self_.level),
                snap.self_.race_id,
                snap.self_.team,
            );
        }
        MiscKind::InitBags => misc::init_bags(iface),
        MiscKind::InitReputations => {
            let snap = iface.get_snapshot();
            reputations::init_reputations(iface, u32::from(snap.self_.level), snap.self_.team);
        }
        MiscKind::InitAmmo => {
            let snap = iface.get_snapshot();
            ammo::init_ammo(iface, snap.self_.class_id, u32::from(snap.self_.level));
        }
        MiscKind::InitInventoryTrade => {
            let snap = iface.get_snapshot();
            inventory_trade::init_inventory_trade(iface, u32::from(snap.self_.level));
        }
        MiscKind::InitSkills => {
            let snap = iface.get_snapshot();
            skills::init_skills(iface, snap.self_.class_id, u32::from(snap.self_.level));
        }
        MiscKind::InitSpecialSpells => {
            special_spells::init_special_spells(iface);
        }
        MiscKind::InitTaxiNodes => {
            let snap = iface.get_snapshot();
            taxi_nodes::init_taxi_nodes(
                iface,
                u32::from(snap.self_.level),
                snap.self_.team,
                snap.self_.pos.map_id,
            );
        }
        MiscKind::InitAvailableSpells => {
            let snap = iface.get_snapshot();
            available_spells::init_available_spells(
                iface,
                snap.self_.class_id,
                u32::from(snap.self_.level),
            );
        }
    }
}
