//! Outfit (named gear set) management — faithful port of PB2 `OutfitAction`
//! + `OutfitListValue`.
//!
//! Outfits persist per-bot in the random-mgr event KV store under the
//! `"outfit list"` key, serialized exactly as PB2 does: each outfit is
//! `name=id,id,...` and the list of outfits is joined with `^`. Reusing the
//! same key and format means data written by either the C++/PB2 side or this
//! Rust port is mutually readable, and persistence (DB-backed) comes for free
//! from the existing event store — no new FFI.
//!
//! Commands (mirrors `OutfitAction::Execute`):
//! * `outfit ?`                 — list saved outfits + usage help
//! * `outfit <name>=id,id,...`  — set a named outfit directly
//! * `outfit <name> +[items]`   — add items to a named outfit
//! * `outfit <name> -[items]`   — remove items from a named outfit
//! * `outfit <name> equip`      — equip the named outfit
//! * `outfit <name> replace`    — unequip current gear, then equip the outfit
//! * `outfit <name> reset`      — clear the named outfit
//! * `outfit <name> update`     — save the currently-equipped items as <name>
//!
//! `[items]` accepts shift-clicked item links (`|Hitem:ID:...|h`) and bare
//! item ids, matching PB2's `ChatHelper::parseItems`.

use cmangos::ItemId;

use super::{reply, PendingCommand};
use crate::bot::state::BotState;

/// KV key under which the `^`-joined outfit list is stored, per PB2.
const OUTFIT_KEY: &str = "outfit list";

/// `EQUIPMENT_SLOT_END` from the fork's `Player.h` — exclusive upper bound
/// of the equipped slots (0..19).
const EQUIPMENT_SLOT_END: u8 = 19;

/// One named gear set.
struct Outfit {
    name: String,
    items: Vec<u32>,
}

/// Parse a single `name=id,id,...` entry. Mirrors PB2
/// `InventoryParseOutfitName` + `InventoryParseOutfitItems`.
fn parse_entry(entry: &str) -> Option<Outfit> {
    let eq = entry.find('=')?;
    let name = entry[..eq].to_string();
    if name.is_empty() {
        return None;
    }
    let items = entry[eq + 1..]
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .filter(|&id| id != 0)
        .collect();
    Some(Outfit { name, items })
}

/// Serialize one outfit back to `name=id,id,...`.
fn serialize_entry(o: &Outfit) -> String {
    let ids: Vec<String> = o.items.iter().map(|i| i.to_string()).collect();
    format!("{}={}", o.name, ids.join(","))
}

/// Load the bot's outfit list from the KV store (entries joined by `^`).
fn load(guid: u32) -> Vec<Outfit> {
    let raw = crate::random_mgr::ffi::kv_get_str(guid, OUTFIT_KEY);
    raw.split('^').filter_map(parse_entry).collect()
}

/// Persist the bot's outfit list to the KV store.
fn store(guid: u32, outfits: &[Outfit]) {
    let joined = outfits
        .iter()
        .map(serialize_entry)
        .collect::<Vec<_>>()
        .join("^");
    crate::random_mgr::ffi::kv_set_str(guid, OUTFIT_KEY, &joined);
}

/// Insert or replace a named outfit. Mirrors PB2 `OutfitAction::Save`: an
/// existing same-named entry is dropped first, and an empty outfit is not
/// re-added (so saving an emptied set removes it).
fn upsert(list: &mut Vec<Outfit>, o: Outfit) {
    list.retain(|x| x.name != o.name);
    if !o.items.is_empty() {
        list.push(o);
    }
}

/// Items of a named outfit, or `None` if absent. Mirrors
/// `InventoryFindOutfitItems`.
fn find<'a>(list: &'a [Outfit], name: &str) -> Option<&'a Outfit> {
    list.iter().find(|o| o.name == name)
}

/// Extract item ids from a chat parameter: pull the id out of every
/// `Hitem:<id>:` link, then collect every bare numeric token. Port of
/// `ChatHelper::parseItemsUnordered` (without the optional DB validation —
/// equip later rejects ids the bot doesn't carry).
pub(crate) fn parse_item_ids(text: &str) -> Vec<u32> {
    let mut ids = Vec::new();
    // Item links: `…|Hitem:12345:0:0…|h[Name]|h|r` — read digits after the marker.
    let mut rest = text;
    while let Some(pos) = rest.find("Hitem:") {
        let after = &rest[pos + "Hitem:".len()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<u32>()
            && n != 0
            && !ids.contains(&n)
        {
            ids.push(n);
        }
        rest = &after[digits.len()..];
    }
    // Bare numeric tokens (a link token is non-numeric as a whole, so its
    // internal id is only picked up by the link scan above).
    for tok in text.split(|c: char| c.is_whitespace() || c == ',') {
        if let Ok(n) = tok.parse::<u32>()
            && n != 0
            && !ids.contains(&n)
        {
            ids.push(n);
        }
    }
    ids
}

/// Equip each item id in order.
fn equip_items(bot: &BotState, ids: &[u32]) {
    for &id in ids {
        bot.interface.equip_item(ItemId(id));
    }
}

/// Move every equipped item back to the bags (PB2 `replace` first half).
fn unequip_all(bot: &BotState) {
    for slot in 0..EQUIPMENT_SLOT_END {
        let id = bot.interface.factory_bot_equipped_item_in_slot(slot);
        if id != 0 {
            bot.interface.unequip_item(ItemId(id));
        }
    }
}

/// The bot's currently-equipped item ids (PB2 `update`).
fn current_equipped(bot: &BotState) -> Vec<u32> {
    let mut items = Vec::new();
    for slot in 0..EQUIPMENT_SLOT_END {
        let id = bot.interface.factory_bot_equipped_item_in_slot(slot);
        if id != 0 && !items.contains(&id) {
            items.push(id);
        }
    }
    items
}

/// List saved outfits to the requester (PB2 `OutfitAction::List`).
fn list_outfits(bot: &BotState, pc: &PendingCommand, guid: u32) {
    for o in load(guid) {
        let ids: Vec<String> = o.items.iter().map(|i| i.to_string()).collect();
        reply(bot, pc, &format!("{}: {}", o.name, ids.join(" ")));
    }
}

/// Handle an `outfit` command. `param` is everything after `outfit`.
pub(crate) fn apply(bot: &mut BotState, pc: &PendingCommand, param: &str) {
    let guid = bot.interface.factory_bot_guid_low();
    let param = param.trim();
    if param.is_empty() {
        return;
    }

    if param == "?" {
        list_outfits(bot, pc, guid);
        reply(bot, pc, "outfit <name> +[item] to add items");
        reply(bot, pc, "outfit <name> -[item] to remove items");
        reply(bot, pc, "outfit <name> equip/replace to equip items");
        return;
    }

    // Direct `name=id,id,...` assignment.
    if let Some(o) = parse_entry(param) {
        let name = o.name.clone();
        let mut list = load(guid);
        upsert(&mut list, o);
        store(guid, &list);
        reply(bot, pc, &format!("Setting outfit {name}"));
        return;
    }

    // `<name> <verb-or-items>`
    let Some(space) = param.find(' ') else {
        return;
    };
    let name = param[..space].to_string();
    let rest = param[space + 1..].trim();

    let mut list = load(guid);

    match rest {
        "equip" => {
            let ids: Vec<u32> = find(&list, &name).map(|o| o.items.clone()).unwrap_or_default();
            equip_items(bot, &ids);
            reply(bot, pc, &format!("Equipping outfit {name}"));
        }
        "replace" => {
            let ids: Vec<u32> = find(&list, &name).map(|o| o.items.clone()).unwrap_or_default();
            unequip_all(bot);
            equip_items(bot, &ids);
            reply(bot, pc, &format!("Replacing current equip with outfit {name}"));
        }
        "reset" => {
            list.retain(|x| x.name != name);
            store(guid, &list);
            reply(bot, pc, &format!("Resetting outfit {name}"));
        }
        "update" => {
            let items = current_equipped(bot);
            upsert(&mut list, Outfit { name: name.clone(), items });
            store(guid, &list);
            reply(bot, pc, &format!("Updating with current items outfit {name}"));
        }
        _ => {
            // `+items` (default) or `-items`.
            let remove = rest.starts_with('-');
            let ids = parse_item_ids(rest);
            if ids.is_empty() {
                return;
            }
            let mut items = find(&list, &name).map(|o| o.items.clone()).unwrap_or_default();
            for id in ids {
                if remove {
                    items.retain(|&x| x != id);
                } else if !items.contains(&id) {
                    items.push(id);
                }
            }
            upsert(&mut list, Outfit { name: name.clone(), items });
            store(guid, &list);
            let verb = if remove { "removed from" } else { "added to" };
            reply(bot, pc, &format!("items {verb} {name}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_entry_round_trips() {
        let o = parse_entry("tank=123,456,789").unwrap();
        assert_eq!(o.name, "tank");
        assert_eq!(o.items, vec![123, 456, 789]);
        assert_eq!(serialize_entry(&o), "tank=123,456,789");
    }

    #[test]
    fn parse_entry_rejects_nameless_and_zero_ids() {
        assert!(parse_entry("=1,2").is_none());
        assert!(parse_entry("noequals").is_none());
        let o = parse_entry("x=0,5,0").unwrap();
        assert_eq!(o.items, vec![5]);
    }

    #[test]
    fn parse_item_ids_handles_links_and_bare() {
        // Bare ids.
        assert_eq!(parse_item_ids("123 456"), vec![123, 456]);
        // Item link — id extracted, internal enchant fields ignored.
        let link = "|cffa335ee|Hitem:19019:0:0:0:0:0:0:0|h[Thunderfury]|h|r";
        assert_eq!(parse_item_ids(link), vec![19019]);
        // Mixed + dedup.
        let mixed = format!("{link} 19019 22691");
        assert_eq!(parse_item_ids(&mixed), vec![19019, 22691]);
    }

    #[test]
    fn upsert_replaces_and_drops_empty() {
        let mut list = Vec::new();
        upsert(&mut list, Outfit { name: "a".into(), items: vec![1, 2] });
        upsert(&mut list, Outfit { name: "a".into(), items: vec![3] });
        assert_eq!(list.len(), 1);
        assert_eq!(find(&list, "a").unwrap().items, vec![3]);
        // Empty outfit removes it.
        upsert(&mut list, Outfit { name: "a".into(), items: vec![] });
        assert!(find(&list, "a").is_none());
    }
}
