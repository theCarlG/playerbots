/// Rogue weapon-poison application.
///
/// Reads `ctx.settings.class_prefs.as_rogue()` and, for each hand with a
/// configured poison that isn't already applied, casts the highest-ranked
/// poison spell the bot knows. Returns `Success` if any poison was cast
/// this tick (so the BT throttle advances), `Failure` otherwise.
///
/// The BT wraps this in a throttle + out-of-combat gate; this function
/// itself does no gating beyond GCD/cooldown checks.
use crate::bot::class_prefs::{PoisonKind, WeaponHand};
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use crate::ffi::SpellId;

pub fn tick_apply_poisons(ctx: &mut TickContext<'_>) -> BtResult {
    let Some(prefs) = ctx.settings.class_prefs.as_rogue() else {
        return BtResult::Failure;
    };

    let mut cast_any = false;
    for (hand, kind) in [
        (WeaponHand::MainHand, prefs.mh),
        (WeaponHand::OffHand, prefs.oh),
    ] {
        let Some(kind) = kind else { continue };
        if ctx.interface.bot_weapon_enchanted(hand.slot_index()) {
            continue;
        }
        let Some(spell) = best_known_rank(ctx, kind) else {
            continue;
        };
        if try_cast(ctx, spell) {
            cast_any = true;
            // One cast per tick — poisons share the GCD and the next hand
            // will be picked up on the next throttled tick.
            break;
        }
    }

    if cast_any {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

/// Walk the rank list from highest → lowest and return the first rank the
/// bot has actually learned.
fn best_known_rank(ctx: &TickContext<'_>, kind: PoisonKind) -> Option<SpellId> {
    kind.ranks()
        .iter()
        .rev()
        .copied()
        .find(|&spell| ctx.interface.knows_spell(spell))
}

/// Local copy of the private `cast` helper in `engine::bt` — targets self
/// because on this server poisons self-target and apply to the equipped
/// weapon via item targeting on the C++ side.
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
