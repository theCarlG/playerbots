/// Retribution Paladin behavior tree (Classic / Vanilla).
///
/// Vanilla ret is seal-twisting / judgement-focused:
/// Priority: emergency shield → hammer of wrath → exorcism → judgement → seal upkeep
use crate::{
    data::spells::vanilla::paladin::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
    ffi::SpellId,
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Emergency: Divine Shield at critical HP
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.20),
            cast_on_current_target(DIVINE_SHIELD),
        ]),

        // Self-heal if low HP and not in emergency range
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.40),
            gcd_gate(cd_gate(FLASH_OF_LIGHT, action(|ctx| {
                let me = ctx.bot_handle;
                if me == 0 { return BtResult::Failure; }
                if ctx.interface.cast_spell(FLASH_OF_LIGHT, me) {
                    ctx.timers.on_spell_cast(FLASH_OF_LIGHT, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Execute phase: Hammer of Wrath
                seq(vec![
                    cond(target_below_20pct),
                    cast_on_current_target(HAMMER_OF_WRATH),
                ]),

                // Holy Shock (talent) — instant damage
                cast_on_current_target(HOLY_SHOCK),

                // Exorcism — instant, undead/demon in vanilla, talented for all
                cast_on_current_target(EXORCISM),

                // Judgement — must have a seal active
                seq(vec![
                    cond(self_has_seal),
                    cast_on_current_target(JUDGEMENT),
                ]),

                // Maintain Seal of Command (talent) or Seal of Righteousness
                seq(vec![
                    cond(|ctx| !self_has_seal(ctx)),
                    gcd_gate(cd_gate(SEAL_OF_COMMAND, action(|ctx| {
                        let me = ctx.bot_handle;
                        if me == 0 { return BtResult::Failure; }
                        if ctx.interface.cast_spell(SEAL_OF_COMMAND, me) {
                            ctx.timers.on_spell_cast(SEAL_OF_COMMAND, ctx.server_time_ms);
                            BtResult::Success
                        } else if ctx.interface.cast_spell(SEAL_OF_RIGHTEOUSNESS, me) {
                            ctx.timers.on_spell_cast(SEAL_OF_RIGHTEOUSNESS, ctx.server_time_ms);
                            BtResult::Success
                        } else {
                            BtResult::Failure
                        }
                    }))),
                ]),

                // Consecration — AoE threat/damage
                cast_on_current_target(CONSECRATION),
            ]),
        ]),
    ])
}

fn target_below_20pct(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    let snap = ctx.interface.get_unit_snapshot(t);
    snap.max_health > 0 && (snap.health as f32 / snap.max_health as f32) < 0.20
}

fn self_has_seal(ctx: &TickContext<'_>) -> bool {
    let me = ctx.bot_handle;
    if me == 0 { return false; }
    // Check all seal spell IDs (multiple ranks)
    ctx.interface.has_aura(me, SEAL_OF_COMMAND)
        || ctx.interface.has_aura(me, SEAL_OF_RIGHTEOUSNESS)
        || ctx.interface.has_aura(me, SpellId(20154)) // Seal of Righteousness rank 1
        || ctx.interface.has_aura(me, SpellId(21082)) // rank 8
}
