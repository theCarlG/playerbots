/// Protection Paladin behavior tree (Classic / Vanilla).
///
/// Prot pala generates threat through seals/judgements/consecration.
/// No taunt spell in vanilla; threat is maintained by Righteous Fury + abilities.
/// Priority: Holy Shield → Exorcism → Seal upkeep → Judgement → Consecration
use crate::{
    data::spells::vanilla::paladin::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
    ffi::SpellId,
};

// Holy Shield: 20925 rank 1, 20927 rank 2, 20928 rank 3, 27179 rank 4
const HOLY_SHIELD: SpellId = SpellId(27179); // rank 4 (talent)
const RIGHTEOUS_FURY: SpellId = SpellId(25780);

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Keep Righteous Fury up (100% increased threat on Holy)
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, RIGHTEOUS_FURY)),
            gcd_gate(cd_gate(RIGHTEOUS_FURY, action(|ctx| {
                let me = ctx.bot_handle;
                if me == 0 { return BtResult::Failure; }
                if ctx.interface.cast_spell(RIGHTEOUS_FURY, me) {
                    ctx.timers.on_spell_cast(RIGHTEOUS_FURY, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Divine Shield at critical HP
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

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Holy Shield — mitigation + damage/threat
                gcd_gate(cd_gate(HOLY_SHIELD, action(|ctx| {
                    let me = ctx.bot_handle;
                    if me == 0 { return BtResult::Failure; }
                    if ctx.interface.cast_spell(HOLY_SHIELD, me) {
                        ctx.timers.on_spell_cast(HOLY_SHIELD, ctx.server_time_ms);
                        BtResult::Success
                    } else {
                        BtResult::Failure
                    }
                }))),

                // Exorcism — instant damage/threat (on undead/demon; talented for all)
                cast_on_current_target(EXORCISM),

                // Hammer of Wrath at execute
                seq(vec![
                    cond(target_below_20pct),
                    cast_on_current_target(HAMMER_OF_WRATH),
                ]),

                // Judgement (requires seal)
                seq(vec![
                    cond(self_has_seal),
                    cast_on_current_target(JUDGEMENT),
                ]),

                // Maintain seal (Seal of Righteousness for single target)
                seq(vec![
                    cond(|ctx| !self_has_seal(ctx)),
                    gcd_gate(cd_gate(SEAL_OF_RIGHTEOUSNESS, action(|ctx| {
                        let me = ctx.bot_handle;
                        if me == 0 { return BtResult::Failure; }
                        if ctx.interface.cast_spell(SEAL_OF_RIGHTEOUSNESS, me) {
                            ctx.timers.on_spell_cast(SEAL_OF_RIGHTEOUSNESS, ctx.server_time_ms);
                            BtResult::Success
                        } else {
                            BtResult::Failure
                        }
                    }))),
                ]),

                // Consecration — AoE threat
                cast_on_current_target(CONSECRATION),

                // Holy Wrath — undead-only AoE
                cast_on_current_target(HOLY_WRATH),
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
    ctx.interface.has_aura(me, SEAL_OF_RIGHTEOUSNESS)
        || ctx.interface.has_aura(me, SEAL_OF_COMMAND)
        || ctx.interface.has_aura(me, SpellId(21082))
        || ctx.interface.has_aura(me, SpellId(20920))
}
