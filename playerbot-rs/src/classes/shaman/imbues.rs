/// Shaman weapon imbues (Rockbiter / Flametongue / Frostbrand / Windfury /
/// Earthliving Weapon).
///
/// Reads `ctx.settings.class_prefs.as_shaman()` and, for each hand whose
/// configured imbue isn't already applied (per `bot_weapon_enchanted`),
/// casts the highest-ranked imbue spell the bot knows. Returns `Success`
/// if any imbue was cast this tick, `Failure` otherwise.
///
/// The BT wraps this in a throttle + out-of-combat gate; this function
/// itself does no gating beyond GCD/cooldown checks. One imbue per tick
/// since they share the GCD.
use crate::bot::class_prefs::{ShamanImbue, WeaponHand};
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use crate::ffi::SpellId;

pub fn tick_apply_shaman_imbues(ctx: &mut TickContext<'_>) -> BtResult {
    let Some(prefs) = ctx.settings.class_prefs.as_shaman() else {
        return BtResult::Failure;
    };

    for (hand, imbue) in [
        (WeaponHand::MainHand, prefs.mh_imbue),
        (WeaponHand::OffHand, prefs.oh_imbue),
    ] {
        let Some(imbue) = imbue else { continue };
        if ctx.interface.bot_weapon_enchanted(hand.slot_index()) {
            continue;
        }
        let Some(spell) = best_known_rank(ctx, imbue) else {
            continue;
        };
        if try_cast(ctx, spell) {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn best_known_rank(ctx: &TickContext<'_>, imbue: ShamanImbue) -> Option<SpellId> {
    imbue
        .ranks()
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
