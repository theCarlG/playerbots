/// Holy Priest behavior tree (Classic / Vanilla).
///
/// Priority: Power Word: Shield → Flash Heal (critical) → Greater Heal (low HP)
///   → Renew (HoT maintenance) → Prayer of Healing (AoE)
use crate::{
    combat::targeting::{find_heal_target, heal_action, heal_party_action, self_buff_action},
    data::spells::vanilla::priest::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cd_gate, cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Power Word: Shield on self when taking heavy damage
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.50 && !ctx.interface.has_aura(ctx.bot_handle, POWER_WORD_SHIELD)),
            self_buff_action(POWER_WORD_SHIELD, |ctx| {
                ctx.self_hp_pct() < 0.50
                    && !ctx.interface.has_aura(ctx.bot_handle, POWER_WORD_SHIELD)
                    && !ctx.interface.has_aura(ctx.bot_handle, 6788) // Weakened Soul
            }),
        ]),

        // Fade when aggro'd
        seq(vec![
            cond(|ctx| !ctx.attackers.is_empty()),
            self_buff_action(FADE, |ctx| !ctx.attackers.is_empty()),
        ]),

        // Power Word: Shield on critical group member
        gcd_gate(cd_gate(POWER_WORD_SHIELD, action(|ctx| {
            if let Some(target) = find_heal_target(ctx, 0.30) {
                if !ctx.interface.has_aura(target, POWER_WORD_SHIELD)
                    && !ctx.interface.has_aura(target, 6788) {
                    if ctx.interface.cast_spell(POWER_WORD_SHIELD, target) {
                        ctx.timers.on_spell_cast(POWER_WORD_SHIELD, ctx.server_time_ms);
                        return BtResult::Success;
                    }
                }
            }
            BtResult::Failure
        }))),

        // Critical heals: Flash Heal
        heal_action(FLASH_HEAL, 0.35),
        heal_party_action(FLASH_HEAL, 0.35),

        // Greater Heal: slow but efficient for sustained low HP
        heal_action(GREATER_HEAL, 0.55),
        heal_party_action(GREATER_HEAL, 0.55),

        // Renew: HoT on group member (not at full HP)
        heal_party_action(RENEW, 0.90),
        heal_action(RENEW, 0.90),

        // Prayer of Healing: AoE heal when multiple people hurt
        seq(vec![
            cond(|ctx| multiple_hurt(ctx, 0.70, 3)),
            heal_party_action(PRAYER_OF_HEALING, 0.70),
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
