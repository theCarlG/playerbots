//! Bot management command dispatch — ports the factory-related `HandleBot*`
//! handlers from `PlayerbotMgr.cpp` into a single Rust entry point.
//!
//! Commands that only need the bot's `World` interface (gear, consumables,
//! enchants, pets, etc.) are handled here. Commands that require additional
//! CMaNGOS globals (`add`, `remove`, `always`, `debug`, etc.) remain in the
//! C++ bridge (`cpp_wrapper/MgrBridge.cpp`).

use crate::config;
use crate::factory::{self, equipment_flags, ConsumableKind, FactoryTransaction};
use cmangos::World;

/// Result of a management bot-command dispatch.
pub enum BotCommandResult {
    /// Rust handled the command; return this message to the player.
    Handled(String),
    /// Rust does not handle this command; fall through to the C++ handler.
    NotHandled,
}

/// Attempt to handle a `.bot <name> <cmd> [param]` command on the Rust side.
///
/// `master_level` is the master player's level (used by `init`); pass the
/// bot's own level when there is no master.
///
/// Returns `Handled(msg)` if the command was dispatched, or `NotHandled` if
/// the C++ side should process it.
pub fn dispatch_bot_command(
    world: &mut dyn World,
    cmd: &str,
    param: &str,
    master_level: u32,
) -> BotCommandResult {
    match cmd {
        "gear" | "equip" => handle_gear(world, param),
        "train" | "learn" => handle_train(world),
        "food" | "drink" => handle_consumable(world, ConsumableKind::Food, "food added"),
        "potions" | "pots" => handle_consumable(world, ConsumableKind::Potions, "potions added"),
        "consumes" | "consumables" | "consums" => {
            handle_consumable(world, ConsumableKind::AdditionalConsumables, "consumables added")
        }
        "regs" | "reg" | "reagents" => {
            handle_consumable(world, ConsumableKind::Reagents, "reagents added")
        }
        "prepare" | "prep" => handle_refresh(world, "consumes/regs added"),
        "init" => handle_init(world, param, master_level),
        "enchants" => handle_enchants(world),
        "ammo" => handle_ammo(world),
        "pet" => handle_pet(world),
        "levelup" | "level" => handle_levelup(world),
        "refresh" => handle_refresh(world, "ok"),
        _ => BotCommandResult::NotHandled,
    }
}

// ── Individual handlers ─────────────────────────────────────────────────

fn handle_gear(world: &mut dyn World, param: &str) -> BotCommandResult {
    let cfg = config::get();
    let progressive_bit = if cfg.random_gear_progression {
        equipment_flags::PROGRESSIVE
    } else {
        0
    };

    let needs_gems;
    let (flags, quality, msg) = match param {
        "" => {
            needs_gems = true;
            (progressive_bit, 0, "random gear equipped")
        }
        "green" | "uncommon" => {
            needs_gems = true;
            (progressive_bit, 2, "random green gear equipped")
        }
        "blue" | "rare" => {
            needs_gems = true;
            (progressive_bit, 3, "random blue gear equipped")
        }
        "purple" | "epic" => {
            needs_gems = true;
            (progressive_bit, 4, "random epic gear equipped")
        }
        "upgrade" => {
            needs_gems = false;
            (
                equipment_flags::INCREMENTAL | progressive_bit,
                0,
                "gear upgraded",
            )
        }
        "sync" => {
            needs_gems = false;
            (
                equipment_flags::SYNC_WITH_MASTER | progressive_bit,
                0,
                "gear upgraded",
            )
        }
        "best" => {
            needs_gems = true;
            (0, 0, "random best gear equipped")
        }
        "partial" => {
            needs_gems = false;
            (
                equipment_flags::PROGRESSIVE | equipment_flags::PARTIAL_UPGRADE,
                0,
                "random gear upgraded to some slots",
            )
        }
        _ => return BotCommandResult::Handled("unknown gear command".into()),
    };

    let mut tx = FactoryTransaction::new(world);
    factory::init_equipment(&mut tx, flags, quality);
    tx.commit();

    if needs_gems {
        let tx = FactoryTransaction::new(world);
        tx.factory_init_all_gems();
        tx.commit();
    }

    BotCommandResult::Handled(msg.into())
}

fn handle_train(world: &mut dyn World) -> BotCommandResult {
    // learnClassLevelSpells is expansion-gated on the C++ side
    // (not available on TBC = MANGOSBOT_ONE). The World callback
    // is a no-op when the expansion doesn't support it.
    world.bot_learn_class_level_spells(false);
    BotCommandResult::Handled("class level spells learned".into())
}

fn handle_consumable(
    world: &mut dyn World,
    kind: ConsumableKind,
    msg: &str,
) -> BotCommandResult {
    let mut tx = FactoryTransaction::new(world);
    factory::init_consumables(&mut tx, kind);
    tx.commit();
    BotCommandResult::Handled(msg.into())
}

fn handle_init(world: &mut dyn World, param: &str, master_level: u32) -> BotCommandResult {
    let (incremental, sync, quality) = match param {
        "" => (true, false, 0u32),
        "white" | "common" => (false, false, 0),
        "green" | "uncommon" => (false, false, 2),
        "blue" | "rare" => (false, false, 3),
        "epic" | "purple" => (false, false, 4),
        "legendary" | "yellow" => (false, false, 5),
        "sync" => (false, true, 5),
        _ => return BotCommandResult::Handled("unknown init command".into()),
    };

    let mut tx = FactoryTransaction::new(world);
    factory::randomize(&mut tx, master_level, incremental, sync, quality);
    tx.commit();
    BotCommandResult::Handled("ok".into())
}

fn handle_enchants(world: &mut dyn World) -> BotCommandResult {
    let tx = FactoryTransaction::new(world);
    tx.factory_enchant_all_equipment();
    tx.commit();
    BotCommandResult::Handled("ok".into())
}

fn handle_ammo(world: &mut dyn World) -> BotCommandResult {
    let mut tx = FactoryTransaction::new(world);
    let snap = tx.get_snapshot();
    let class = snap.self_.class_id;
    let level = u32::from(snap.self_.level);
    factory::ammo::init_ammo(&mut tx, class, level);
    tx.commit();
    BotCommandResult::Handled("ok".into())
}

fn handle_pet(world: &mut dyn World) -> BotCommandResult {
    let mut tx = FactoryTransaction::new(world);
    factory::pet::init_pet(&mut tx);
    tx.commit();

    let mut tx = FactoryTransaction::new(world);
    factory::pet::init_pet_spells(&mut tx);
    tx.commit();
    BotCommandResult::Handled("ok".into())
}

fn handle_levelup(world: &mut dyn World) -> BotCommandResult {
    let level = u32::from(world.get_snapshot().self_.level);
    let mut tx = FactoryTransaction::new(world);
    factory::randomize(&mut tx, level, true, false, 0);
    tx.commit();
    BotCommandResult::Handled("ok".into())
}

fn handle_refresh(world: &mut dyn World, msg: &str) -> BotCommandResult {
    let mut tx = FactoryTransaction::new(world);
    factory::refresh(&mut tx);
    tx.commit();
    BotCommandResult::Handled(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockWorld;

    #[test]
    fn unknown_command_returns_not_handled() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "foobar", "", 60) {
            BotCommandResult::NotHandled => {}
            BotCommandResult::Handled(msg) => panic!("expected NotHandled, got Handled({msg})"),
        }
    }

    #[test]
    fn gear_unknown_subcommand_returns_error() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "gear", "nonsense", 60) {
            BotCommandResult::Handled(msg) => assert_eq!(msg, "unknown gear command"),
            BotCommandResult::NotHandled => panic!("expected Handled"),
        }
    }

    #[test]
    fn init_unknown_subcommand_returns_error() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "init", "nonsense", 60) {
            BotCommandResult::Handled(msg) => assert_eq!(msg, "unknown init command"),
            BotCommandResult::NotHandled => panic!("expected Handled"),
        }
    }

    #[test]
    fn add_command_falls_through_to_cpp() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "add", "", 60) {
            BotCommandResult::NotHandled => {}
            BotCommandResult::Handled(_) => panic!("expected NotHandled for 'add'"),
        }
    }

    #[test]
    fn always_command_falls_through_to_cpp() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "always", "", 60) {
            BotCommandResult::NotHandled => {}
            BotCommandResult::Handled(_) => panic!("expected NotHandled for 'always'"),
        }
    }

    #[test]
    fn food_returns_handled() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "food", "", 60) {
            BotCommandResult::Handled(msg) => assert_eq!(msg, "food added"),
            BotCommandResult::NotHandled => panic!("expected Handled for 'food'"),
        }
    }

    #[test]
    fn potions_returns_handled() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "potions", "", 60) {
            BotCommandResult::Handled(msg) => assert_eq!(msg, "potions added"),
            BotCommandResult::NotHandled => panic!("expected Handled for 'potions'"),
        }
    }

    #[test]
    fn train_returns_handled() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "train", "", 60) {
            BotCommandResult::Handled(msg) => assert_eq!(msg, "class level spells learned"),
            BotCommandResult::NotHandled => panic!("expected Handled for 'train'"),
        }
    }

    #[test]
    fn refresh_returns_ok() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "refresh", "", 60) {
            BotCommandResult::Handled(msg) => assert_eq!(msg, "ok"),
            BotCommandResult::NotHandled => panic!("expected Handled for 'refresh'"),
        }
    }

    #[test]
    fn gear_sync_returns_handled() {
        let mut w = MockWorld::new();
        match dispatch_bot_command(&mut w, "gear", "sync", 60) {
            BotCommandResult::Handled(msg) => assert_eq!(msg, "gear upgraded"),
            BotCommandResult::NotHandled => panic!("expected Handled for 'gear sync'"),
        }
    }
}
