/// Shaman configured-totem drop.
///
/// Reads `ctx.settings.class_prefs.as_shaman()` and, for each slot with a
/// configured role whose slot is currently empty (per
/// `World::bot_active_totem_mask`), casts the highest-ranked totem
/// spell the bot knows for that role. Returns `Success` if any totem was
/// cast this tick, `Failure` otherwise.
///
/// The BT wraps this in a throttle so the check runs on a ~2s cadence; this
/// function itself does no gating beyond GCD/cooldown checks. Only one
/// totem is dropped per tick (they share the GCD).
use crate::bot::class_prefs::{TotemRole, TotemSlot};
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use cmangos::SpellId;

pub fn tick_drop_configured_totems(ctx: &mut TickContext<'_>) -> BtResult {
    let Some(prefs) = ctx.settings.class_prefs.as_shaman() else {
        return BtResult::Failure;
    };

    let mask = ctx.interface.bot_active_totem_mask();
    for slot in [
        TotemSlot::Earth,
        TotemSlot::Fire,
        TotemSlot::Water,
        TotemSlot::Air,
    ] {
        let Some(role) = prefs.get(slot) else {
            continue;
        };
        if mask & slot_bit(slot) != 0 {
            // Slot already filled.
            continue;
        }
        let Some(spell) = best_known_rank(ctx, role) else {
            continue;
        };
        if try_cast(ctx, spell) {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

/// `bot_active_totem_mask` bit layout: 0=fire, 1=earth, 2=water, 3=air.
fn slot_bit(slot: TotemSlot) -> u8 {
    match slot {
        TotemSlot::Fire => 1 << 0,
        TotemSlot::Earth => 1 << 1,
        TotemSlot::Water => 1 << 2,
        TotemSlot::Air => 1 << 3,
    }
}

fn best_known_rank(ctx: &TickContext<'_>, role: TotemRole) -> Option<SpellId> {
    role.ranks()
        .iter()
        .rev()
        .copied()
        .find(|&spell| ctx.interface.knows_spell(spell))
}

fn try_cast(ctx: &mut TickContext<'_>, spell: SpellId) -> bool {
    if ctx.timers.gcd_active(ctx.server_time_ms) {
        return false;
    }
    if ctx.timers.spell_on_cooldown(spell, ctx.server_time_ms) {
        return false;
    }
    if ctx.interface.cast_spell(spell, ctx.bot_handle) {
        ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
        true
    } else {
        false
    }
}
