/// Paladin player-configurable auras and blessings.
///
/// Reads `ctx.settings.class_prefs.as_paladin()`. Two independent tick
/// handlers:
///
/// * `tick_maintain_paladin_aura` — if the configured aura is not the
///   currently active self-aura, cast the highest-ranked version the bot
///   knows. (Paladin auras are self-exclusive, so any prior aura is
///   overwritten automatically.)
/// * `tick_apply_paladin_blessings` — for each party member (and self)
///   missing the configured blessing, cast the best-known rank. Prefers
///   the Greater variant when `prefs.use_greater` is set and the bot
///   knows it.
///
/// Both handlers do no gating beyond GCD/cooldown checks; the BT wraps
/// them in throttles + out-of-combat gates as appropriate.
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use cmangos::SpellId;

pub fn tick_maintain_paladin_aura(ctx: &mut TickContext<'_>) -> BtResult {
    let Some(prefs) = ctx.settings.class_prefs.as_paladin() else {
        return BtResult::Failure;
    };
    let Some(aura) = prefs.aura else {
        return BtResult::Failure;
    };
    let Some(spell) = best_known_rank(ctx, aura.ranks()) else {
        return BtResult::Failure;
    };
    // Already active? (Auras are self-exclusive — the active one's buff
    // id matches the top-rank spell id we just resolved.)
    if ctx.interface.has_aura(ctx.bot_handle, spell) {
        return BtResult::Failure;
    }
    if try_cast_self(ctx, spell) {
        BtResult::Success
    } else {
        BtResult::Failure
    }
}

pub fn tick_apply_paladin_blessings(ctx: &mut TickContext<'_>) -> BtResult {
    let Some(prefs) = ctx.settings.class_prefs.as_paladin() else {
        return BtResult::Failure;
    };
    let Some(blessing) = prefs.blessing else {
        return BtResult::Failure;
    };

    // Resolve best known rank — greater first if configured and known,
    // else single-target.
    let (spell, _is_greater) = if prefs.use_greater
        && let Some(s) = best_known_rank(ctx, blessing.greater_ranks())
    {
        (s, true)
    } else if let Some(s) = best_known_rank(ctx, blessing.ranks()) {
        (s, false)
    } else {
        return BtResult::Failure;
    };

    // Find any party member (including self) missing the blessing buff.
    let me = ctx.bot_handle;
    let group_size = ctx.snap.group_size as usize;
    let candidates = std::iter::once(me).chain(
        ctx.snap.group_members[..group_size]
            .iter()
            .copied()
            .filter(|&h| h != 0 && h != me),
    );
    for target in candidates {
        // Blessing spell id is the same as the buff aura id.
        if ctx.interface.has_aura(target, spell) {
            continue;
        }
        if try_cast(ctx, spell, target) {
            return BtResult::Success;
        }
    }
    BtResult::Failure
}

fn best_known_rank(ctx: &TickContext<'_>, ranks: &[SpellId]) -> Option<SpellId> {
    ranks
        .iter()
        .rev()
        .copied()
        .find(|&spell| ctx.interface.knows_spell(spell))
}

fn try_cast_self(ctx: &mut TickContext<'_>, spell: SpellId) -> bool {
    try_cast(ctx, spell, ctx.bot_handle)
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
