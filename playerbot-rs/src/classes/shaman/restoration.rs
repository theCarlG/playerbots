/// Restoration Shaman behavior tree (Classic / Vanilla).
///
/// Priority: Mana Spring Totem → critical Healing Wave → Chain Heal (AoE) →
///   Lesser Healing Wave (quick) → Healing Wave (sustained)
use crate::{
    combat::targeting::{heal_action, heal_party_action},
    data::spells::vanilla::shaman::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cd_gate, cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Mana Spring Totem — mana regen for group
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, MANA_SPRING_TOTEM)
                && ctx.snap.group_size > 1),
            gcd_gate(cd_gate(MANA_SPRING_TOTEM, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(MANA_SPRING_TOTEM, me) {
                    ctx.timers.on_spell_cast(MANA_SPRING_TOTEM, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Nature's Swiftness for instant critical heal
        seq(vec![
            cond(|ctx| {
                use crate::combat::targeting::any_group_member_below;
                any_group_member_below(ctx, 0.20)
                    && ctx.interface.has_aura(ctx.bot_handle, NATURE_SWIFTNESS)
            }),
            heal_action(HEALING_WAVE, 0.20),
        ]),

        // Critical heals
        heal_action(HEALING_WAVE, 0.30),
        heal_party_action(HEALING_WAVE, 0.30),

        // Lesser Healing Wave — fast, smaller heal
        heal_action(LESSER_HEALING_WAVE, 0.55),
        heal_party_action(LESSER_HEALING_WAVE, 0.55),

        // Chain Heal — AoE when multiple hurt
        seq(vec![
            cond(|ctx| multiple_hurt(ctx, 0.75, 2)),
            heal_party_action(CHAIN_HEAL, 0.75),
        ]),

        // Sustained healing
        heal_action(HEALING_WAVE, 0.80),
        heal_party_action(HEALING_WAVE, 0.80),

        // Purge (remove buffs from enemies)
        seq(vec![
            cond(|ctx| ctx.in_combat()),
            gcd_gate(cd_gate(PURGE, action(|ctx| {
                let Some(target) = ctx.current_target() else { return BtResult::Failure };
                if ctx.interface.cast_spell(PURGE, target) {
                    ctx.timers.on_spell_cast(PURGE, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),
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
