/// Fire Mage behavior tree (Classic / Vanilla).
///
/// Priority: Counterspell → Pyroblast (talent opener) → Scorch (maintain stacks) →
///   Fireball → Fire Blast (instant)
use crate::{
    data::spells::vanilla::mage::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

// Fire Vulnerability stacks (from Improved Scorch talent):
// aura 22959 (rank 5 = max stacks)
const FIRE_VULNERABILITY: u32 = 22959;
const FIRE_VULNERABILITY_STACKS: u8 = 5;

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

        // Evocation
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
                // Counterspell
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.get_unit_snapshot(t).is_casting)),
                    cast_on_current_target(COUNTERSPELL),
                ]),

                // Fire Blast execute phase
                seq(vec![
                    cond(target_below_20pct),
                    cast_on_current_target(FIRE_BLAST),
                ]),

                // Scorch if Fire Vulnerability < 5 stacks
                seq(vec![
                    cond(target_needs_scorch),
                    cast_on_current_target(SCORCH),
                ]),

                // Fireball — main nuke
                cast_on_current_target(FIREBALL),

                // Scorch filler (if mana efficient)
                cast_on_current_target(SCORCH),
            ]),
        ]),
    ])
}

fn target_below_20pct(ctx: &TickContext<'_>) -> bool {
    ctx.current_target().map_or(false, |t| {
        let s = ctx.interface.get_unit_snapshot(t);
        s.max_health > 0 && (s.health as f32 / s.max_health as f32) < 0.20
    })
}

fn target_needs_scorch(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    let aura = ctx.interface.get_aura(t, FIRE_VULNERABILITY);
    // Apply if no fire vuln aura or stacks < max
    aura.map_or(true, |a| a.stacks < FIRE_VULNERABILITY_STACKS)
}
