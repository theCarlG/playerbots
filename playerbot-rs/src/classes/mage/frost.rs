/// Frost Mage behavior tree (Classic / Vanilla).
///
/// Priority: Ice Block (emergency) → Frost Nova (melee range) → Cone of Cold →
///   Counterspell → Frostbolt
use crate::{
    data::spells::vanilla::mage::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

// Cold Snap resets frost CD: 11958 (rank 1 talent)
const COLD_SNAP: u32 = 12472;

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Ice Block at critical HP
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.20),
            gcd_gate(cd_gate(ICE_BLOCK, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(ICE_BLOCK, me) {
                    ctx.timers.on_spell_cast(ICE_BLOCK, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Evocation when very low mana
        seq(vec![
            cond(|ctx| ctx.self_mana_pct() < 0.15),
            gcd_gate(cd_gate(EVOCATION, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(EVOCATION, me) {
                    ctx.timers.on_spell_cast(EVOCATION, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Counterspell — interrupt enemy cast
                seq(vec![
                    cond(target_is_casting),
                    cast_on_current_target(COUNTERSPELL),
                ]),

                // Frost Nova + Blink if enemy in melee range
                seq(vec![
                    cond(enemy_melee_range),
                    sel(vec![
                        cast_on_current_target(FROST_NOVA),
                        gcd_gate(cd_gate(BLINK, action(|ctx| {
                            let me = ctx.bot_handle;
                            if ctx.interface.cast_spell(BLINK, me) {
                                ctx.timers.on_spell_cast(BLINK, ctx.server_time_ms);
                                BtResult::Success
                            } else {
                                BtResult::Failure
                            }
                        }))),
                    ]),
                ]),

                // Cone of Cold (after Nova, target frozen)
                seq(vec![
                    cond(target_is_frozen),
                    cast_on_current_target(CONE_OF_COLD),
                ]),

                // Frostbolt — main nuke
                cast_on_current_target(FROSTBOLT),

                // Fire Blast — instant execute
                seq(vec![
                    cond(target_below_20pct),
                    cast_on_current_target(FIRE_BLAST),
                ]),
            ]),
        ]),
    ])
}

fn target_is_casting(ctx: &TickContext<'_>) -> bool {
    ctx.current_target().map_or(false, |t| ctx.interface.get_unit_snapshot(t).is_casting)
}

fn enemy_melee_range(ctx: &TickContext<'_>) -> bool {
    ctx.current_target().map_or(false, |t| ctx.interface.unit_distance(t) < 5.0)
}

fn target_is_frozen(ctx: &TickContext<'_>) -> bool {
    // Frozen = has Frost Nova (42397 or 122) or Freeze (33395) aura
    ctx.current_target().map_or(false, |t|
        ctx.interface.has_aura(t, 122)
            || ctx.interface.has_aura(t, 42397))
}

fn target_below_20pct(ctx: &TickContext<'_>) -> bool {
    ctx.current_target().map_or(false, |t| {
        let s = ctx.interface.get_unit_snapshot(t);
        s.max_health > 0 && (s.health as f32 / s.max_health as f32) < 0.20
    })
}
