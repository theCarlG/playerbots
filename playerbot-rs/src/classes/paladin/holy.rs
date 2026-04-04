/// Holy Paladin behavior tree (Classic / Vanilla).
///
/// Priority: emergency Lay on Hands → critical heals → medium heals → light heals
use crate::{
    combat::targeting::{find_heal_target, heal_action, heal_party_action},
    data::spells::vanilla::paladin::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cd_gate, cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Lay on Hands: emergency on any group member at critical HP
        gcd_gate(cd_gate(LAY_ON_HANDS, action(|ctx| {
            if let Some(target) = find_heal_target(ctx, 0.10) {
                if ctx.interface.cast_spell(LAY_ON_HANDS, target) {
                    ctx.timers.on_spell_cast(LAY_ON_HANDS, ctx.server_time_ms);
                    return BtResult::Success;
                }
            }
            BtResult::Failure
        }))),

        // Divine Shield self when critical
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.15),
            gcd_gate(cd_gate(DIVINE_SHIELD, action(|ctx| {
                let me = ctx.bot_handle;
                if me == 0 { return BtResult::Failure; }
                if ctx.interface.cast_spell(DIVINE_SHIELD, me) {
                    ctx.timers.on_spell_cast(DIVINE_SHIELD, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Critical heals: below 30% HP
        heal_action(HOLY_LIGHT, 0.30),
        heal_party_action(HOLY_LIGHT, 0.30),

        // Holy Shock (talent) — instant critical heal
        heal_action(HOLY_SHOCK, 0.40),
        heal_party_action(HOLY_SHOCK, 0.40),

        // Medium heals: below 65% HP
        heal_action(HOLY_LIGHT, 0.65),
        heal_party_action(HOLY_LIGHT, 0.65),

        // Light heals: Flash of Light for efficiency
        heal_action(FLASH_OF_LIGHT, 0.85),
        heal_party_action(FLASH_OF_LIGHT, 0.85),

        // Maintain Blessing of Wisdom (self, when no blessing)
        seq(vec![
            cond(self_needs_blessing),
            gcd_gate(cd_gate(BLESSING_OF_WISDOM, action(|ctx| {
                let me = ctx.bot_handle;
                if me == 0 { return BtResult::Failure; }
                if ctx.interface.cast_spell(BLESSING_OF_WISDOM, me) {
                    ctx.timers.on_spell_cast(BLESSING_OF_WISDOM, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),
    ])
}

fn self_needs_blessing(ctx: &TickContext<'_>) -> bool {
    let me = ctx.bot_handle;
    if me == 0 { return false; }
    !ctx.interface.has_aura(me, BLESSING_OF_WISDOM)
        && !ctx.interface.has_aura(me, BLESSING_OF_MIGHT)
        && !ctx.interface.has_aura(me, BLESSING_OF_KINGS)
        && !ctx.interface.has_aura(me, 19742) // Greater Blessing of Wisdom
}
