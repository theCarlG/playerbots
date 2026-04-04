/// Discipline Priest behavior tree (Classic / Vanilla).
///
/// Vanilla Disc is a hybrid — prioritizes Power Word: Shield and Inner Fire,
/// heals like Holy but with more mitigation focus.
use crate::{
    combat::targeting::{heal_action, heal_party_action, self_buff_action},
    data::spells::vanilla::priest::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cd_gate, cond, gcd_gate, sel, seq},
        context::TickContext,
    },
    ffi::SpellId,
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Inner Fire — mana-efficient self buff
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, INNER_FIRE)),
            self_buff_action(INNER_FIRE, |ctx| !ctx.interface.has_aura(ctx.bot_handle, INNER_FIRE)),
        ]),

        // Fade when aggro'd
        seq(vec![
            cond(|ctx| !ctx.attackers.is_empty()),
            self_buff_action(FADE, |ctx| !ctx.attackers.is_empty()),
        ]),

        // Power Word: Shield — proactive mitigation
        gcd_gate(cd_gate(POWER_WORD_SHIELD, action(|ctx| {
            use crate::combat::targeting::find_heal_target;
            if let Some(target) = find_heal_target(ctx, 0.80) {
                if !ctx.interface.has_aura(target, POWER_WORD_SHIELD)
                    && !ctx.interface.has_aura(target, SpellId(6788)) {
                    if ctx.interface.cast_spell(POWER_WORD_SHIELD, target) {
                        ctx.timers.on_spell_cast(POWER_WORD_SHIELD, ctx.server_time_ms);
                        return BtResult::Success;
                    }
                }
            }
            BtResult::Failure
        }))),

        // Flash Heal: critical
        heal_action(FLASH_HEAL, 0.40),
        heal_party_action(FLASH_HEAL, 0.40),

        // Greater Heal: sustained healing
        heal_action(GREATER_HEAL, 0.65),
        heal_party_action(GREATER_HEAL, 0.65),

        // Renew: HoT maintenance
        heal_party_action(RENEW, 0.85),
        heal_action(RENEW, 0.85),
    ])
}
