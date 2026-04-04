/// Balance Druid behavior tree (Classic / Vanilla).
///
/// Priority: Faerie Fire → Insect Swarm → Moonfire → Starfire → Wrath (filler)
use crate::{
    combat::targeting::self_buff_action,
    data::spells::vanilla::druid::*,
    engine::{
        bt_nodes::{BtNode, cast_on_current_target, cond, sel, seq},
        context::TickContext,
    },
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Self-heal when low HP (Balance can cast in caster form)
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.35),
            sel(vec![
                cast_heal_self(),
                cast_on_current_target(BARKSKIN),
            ]),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Faerie Fire — armor debuff (keep up)
                seq(vec![
                    cond(target_needs_faerie_fire),
                    cast_on_current_target(FAERIE_FIRE_FERAL), // uses same ID
                ]),

                // Insect Swarm — nature DoT (talent)
                seq(vec![
                    cond(target_needs_insect_swarm),
                    cast_on_current_target(INSECT_SWARM),
                ]),

                // Moonfire — arcane DoT + nuke
                seq(vec![
                    cond(target_needs_moonfire),
                    cast_on_current_target(MOONFIRE),
                ]),

                // Starfire — main nuke
                cast_on_current_target(STARFIRE),

                // Wrath — filler (shorter cast, useful while moving)
                cast_on_current_target(WRATH),
            ]),
        ]),
    ])
}

fn cast_heal_self() -> Box<dyn BtNode> {
    use crate::engine::bt_nodes::{BtResult, action, cd_gate, gcd_gate};
    gcd_gate(cd_gate(REGROWTH, action(|ctx| {
        let me = ctx.bot_handle;
        if me == 0 { return BtResult::Failure; }
        if ctx.interface.cast_spell(REGROWTH, me) {
            ctx.timers.on_spell_cast(REGROWTH, ctx.server_time_ms);
            BtResult::Success
        } else {
            BtResult::Failure
        }
    })))
}

fn target_needs_faerie_fire(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    !ctx.interface.has_aura(t, FAERIE_FIRE_FERAL) && !ctx.interface.has_aura(t, 770)
}

fn target_needs_insect_swarm(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    !ctx.interface.has_aura(t, INSECT_SWARM) && !ctx.interface.has_aura(t, 24977)
}

fn target_needs_moonfire(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    !ctx.interface.has_aura(t, MOONFIRE) && !ctx.interface.has_aura(t, 26987)
}
