/// Warrior forced-stance enforcement.
///
/// Reads `ctx.settings.class_prefs.as_warrior()`. If `forced_stance` is
/// `Some(stance)` and the bot is not already in that stance, casts the
/// stance spell. This is a BT override used for situations where a raid
/// leader wants every warrior locked into e.g. Berserker for a fear-heavy
/// fight; normal rotation stance swaps are disabled upstream whenever
/// `forced_stance` is set.
///
/// "Am I already in this stance?" is answered by `has_aura(self,
/// stance_spell)` — on this server each stance spell places a
/// same-id self-buff while active.
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;

pub fn tick_enforce_warrior_stance(ctx: &mut TickContext<'_>) -> BtResult {
    let Some(prefs) = ctx.settings.class_prefs.as_warrior() else {
        return BtResult::Failure;
    };
    let Some(stance) = prefs.forced_stance else {
        return BtResult::Failure;
    };
    let spell = stance.spell();
    if ctx.interface.has_aura(ctx.bot_handle, spell) {
        return BtResult::Failure;
    }
    if !ctx.interface.knows_spell(spell) {
        return BtResult::Failure;
    }
    if ctx.timers.gcd_active(ctx.server_time_ms) {
        return BtResult::Failure;
    }
    if ctx.timers.spell_on_cooldown(spell, ctx.server_time_ms) {
        return BtResult::Failure;
    }
    if ctx.interface.cast_spell(spell, ctx.bot_handle) {
        ctx.timers.on_spell_cast(spell, ctx.server_time_ms);
        BtResult::Success
    } else {
        BtResult::Failure
    }
}
