/// Restoration Druid behavior tree (Classic / Vanilla).
///
/// Priority: Rebirth → Regrowth (critical) → Healing Touch (critical) →
///   Rejuvenation (HoT maintenance) → Tranquility (AoE) → Innervate (OOM)
use crate::{
    combat::targeting::{find_heal_target, heal_action, heal_party_action},
    data::spells::vanilla::druid::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cd_gate, cond, gcd_gate, sel, seq},
        context::TickContext,
    },
    ffi::SpellId,
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Rebirth — combat res on dead group member (critical utility)
        gcd_gate(cd_gate(SpellId(0), action(|_| BtResult::Failure))), // placeholder for rebirth logic

        // Innervate self when very low mana
        seq(vec![
            cond(|ctx| ctx.self_mana_pct() < 0.10),
            gcd_gate(cd_gate(INNERVATE, action(|ctx| {
                let me = ctx.bot_handle;
                if me == 0 { return BtResult::Failure; }
                if ctx.interface.cast_spell(INNERVATE, me) {
                    ctx.timers.on_spell_cast(INNERVATE, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Barkskin — self mitigation when taking damage
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.40 && !ctx.attackers.is_empty()),
            gcd_gate(cd_gate(BARKSKIN, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(BARKSKIN, me) {
                    ctx.timers.on_spell_cast(BARKSKIN, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Critical heals: Regrowth (HoT + direct) or Healing Touch
        heal_action(REGROWTH, 0.30),
        heal_party_action(REGROWTH, 0.30),
        heal_action(HEALING_TOUCH, 0.35),
        heal_party_action(HEALING_TOUCH, 0.35),

        // Medium heals: Regrowth
        heal_action(REGROWTH, 0.60),
        heal_party_action(REGROWTH, 0.60),

        // Tranquility — AoE heal when multiple people hurt
        seq(vec![
            cond(|ctx| multiple_hurt(ctx, 0.60, 3)),
            gcd_gate(cd_gate(TRANQUILITY, action(|ctx| {
                // Self-cast channel — heals nearby group members
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(TRANQUILITY, me) {
                    ctx.timers.on_spell_cast(TRANQUILITY, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Rejuvenation — HoT maintenance
        heal_party_action(REJUVENATION, 0.90),
        heal_action(REJUVENATION, 0.90),
    ])
}

fn multiple_hurt(ctx: &TickContext<'_>, threshold: f32, min_count: usize) -> bool {
    let mut count = 0;
    if ctx.self_hp_pct() < threshold { count += 1; }
    for i in 0..ctx.snap.group_size as usize {
        let h = ctx.snap.group_members[i];
        if h == 0 || h == ctx.bot_handle { continue; }
        let snap = ctx.interface.get_unit_snapshot(h);
        if snap.is_alive && snap.max_health > 0
            && (snap.health as f32 / snap.max_health as f32) < threshold {
            count += 1;
        }
    }
    count >= min_count
}
