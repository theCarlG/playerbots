/// Warlock player-configurable Curse maintenance on the current target.
///
/// Reads `ctx.settings.class_prefs.as_warlock()`. If the current combat
/// target does not already have the configured curse, casts the best
/// known rank on it. Curses are mutually exclusive on a single target,
/// so only one is ever up at a time — the player's choice decides which.
use crate::bot::class_prefs::WarlockCurse;
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use cmangos::SpellId;

pub fn tick_maintain_warlock_curse(ctx: &mut TickContext<'_>) -> BtResult {
    let Some(prefs) = ctx.settings.class_prefs.as_warlock() else {
        return BtResult::Failure;
    };
    let Some(curse) = prefs.curse else {
        return BtResult::Failure;
    };
    let Some(target) = ctx.current_target() else {
        return BtResult::Failure;
    };
    let Some(spell) = best_known_rank(ctx, curse) else {
        return BtResult::Failure;
    };
    if ctx.interface.has_aura(target, spell) {
        return BtResult::Failure;
    }
    if try_cast(ctx, spell, target) {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

fn best_known_rank(ctx: &TickContext<'_>, curse: WarlockCurse) -> Option<SpellId> {
    curse
        .ranks()
        .iter()
        .rev()
        .copied()
        .find(|&spell| ctx.interface.knows_spell(spell))
}

fn try_cast(ctx: &mut TickContext<'_>, spell: SpellId, target: u64) -> bool {
    if ctx.timers.gcd_active(ctx.server_time_ms) {
        return false;
    }
    if ctx.timers.spell_on_cooldown(spell, ctx.server_time_ms) {
        return false;
    }
    if ctx.interface.cast_spell(spell, target) {
        ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
        true
    } else {
        false
    }
}
