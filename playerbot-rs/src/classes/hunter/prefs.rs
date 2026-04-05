/// Hunter player-configurable Aspect maintenance.
///
/// Reads `ctx.settings.class_prefs.as_hunter()`. If the configured Aspect
/// is not currently active on the bot, casts the highest-ranked version
/// the bot knows. Aspects are self-exclusive, so any previously active
/// Aspect is overwritten automatically.
///
/// Trap placement is not handled here — that happens inside the combat
/// rotation when a crowd-control target is selected. This tick handler
/// only maintains the always-on Aspect choice.
use crate::bot::class_prefs::HunterAspect;
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use crate::ffi::SpellId;

pub fn tick_maintain_hunter_aspect(ctx: &mut TickContext<'_>) -> BtResult {
    let Some(prefs) = ctx.settings.class_prefs.as_hunter() else {
        return BtResult::Failure;
    };
    let Some(aspect) = prefs.aspect else {
        return BtResult::Failure;
    };
    let Some(spell) = best_known_rank(ctx, aspect) else {
        return BtResult::Failure;
    };
    if ctx.interface.has_aura(ctx.bot_handle, spell) {
        return BtResult::Failure;
    }
    if try_cast(ctx, spell) {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn best_known_rank(ctx: &TickContext<'_>, aspect: HunterAspect) -> Option<SpellId> {
    aspect
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
