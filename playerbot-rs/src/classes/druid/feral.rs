/// Feral Druid behavior tree (Classic / Vanilla).
///
/// Handles both DPS (Cat Form) and Tank (Bear Form) based on role.
/// DPS: Cat Form → Rake → Shred → Rip/Ferocious Bite → Claw (builder)
/// Tank: Bear Form → Growl → Maul → Demoralizing Roar → Swipe
use crate::{
    data::spells::vanilla::druid::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
    ffi::SpellId,
};

// Combo point tracking isn't available directly — we approximate via aura checks.
// Rip and Ferocious Bite are finishers (cast when we have enough CPs from context).
// In practice, the server tracks combo points and can_cast will return false for
// finishers at 0 CPs.

// Bear form specific spells
const GROWL: SpellId = SpellId(6795);
const FRENZIED_REGENERATION: SpellId = SpellId(22842); // rank 3

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Tank mode: maintain Bear Form
        seq(vec![
            cond(is_tank_role),
            bear_tank_tree(),
        ]),

        // DPS mode: cat form
        seq(vec![
            cond(|ctx| !is_tank_role(ctx)),
            cat_dps_tree(),
        ]),
    ])
}

fn bear_tank_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Ensure Bear Form
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, BEAR_FORM)),
            gcd_gate(cd_gate(BEAR_FORM, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(BEAR_FORM, me) {
                    ctx.timers.on_spell_cast(BEAR_FORM, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Self-heal: Frenzied Regeneration at low HP
                seq(vec![
                    cond(|ctx| ctx.self_hp_pct() < 0.30),
                    gcd_gate(cd_gate(FRENZIED_REGENERATION, action(|ctx| {
                        let me = ctx.bot_handle;
                        if ctx.interface.cast_spell(FRENZIED_REGENERATION, me) {
                            ctx.timers.on_spell_cast(FRENZIED_REGENERATION, ctx.server_time_ms);
                            BtResult::Success
                        } else {
                            BtResult::Failure
                        }
                    }))),
                ]),

                // Growl — taunt
                cast_on_current_target(GROWL),

                // Faerie Fire (Feral) — armor reduction
                seq(vec![
                    cond(target_needs_faerie_fire),
                    cast_on_current_target(FAERIE_FIRE_FERAL),
                ]),

                // Demoralizing Roar — attack power reduction
                seq(vec![
                    cond(target_needs_demo_roar),
                    cast_on_current_target(DEMORALIZING_ROAR),
                ]),

                // Maul — rage dump (queues on next melee swing, like Heroic Strike)
                cast_on_current_target(MAUL),

                // Swipe — AoE
                cast_on_current_target(SWIPE_BEAR),
            ]),
        ]),
    ])
}

fn cat_dps_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Ensure Cat Form
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, CAT_FORM)),
            gcd_gate(cd_gate(CAT_FORM, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(CAT_FORM, me) {
                    ctx.timers.on_spell_cast(CAT_FORM, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Finisher: Ferocious Bite (burst) — only at high CP (can_cast checks)
                cast_on_current_target(FEROCIOUS_BITE),

                // Finisher: Rip — DoT (better if target lives long)
                seq(vec![
                    cond(target_needs_rip),
                    cast_on_current_target(RIP),
                ]),

                // Rake — DoT builder
                seq(vec![
                    cond(target_needs_rake),
                    cast_on_current_target(RAKE),
                ]),

                // Shred — high damage builder (requires behind target; can_cast handles this)
                cast_on_current_target(SHRED),

                // Claw — alternative builder when not behind
                cast_on_current_target(CLAW),
            ]),
        ]),
    ])
}

fn is_tank_role(ctx: &TickContext<'_>) -> bool {
    ctx.snap.self_.role & 1 != 0 // TANK bit
}

fn target_needs_faerie_fire(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    use crate::engine::aura_helpers::{has_any_rank, FAERIE_FIRE_RANKS};
    !has_any_rank(ctx.interface, t, FAERIE_FIRE_RANKS)
}

fn target_needs_demo_roar(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    use crate::engine::aura_helpers::{has_any_rank, DEMO_ROAR_RANKS};
    !has_any_rank(ctx.interface, t, DEMO_ROAR_RANKS)
}

fn target_needs_rip(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    use crate::engine::aura_helpers::{has_any_rank, RIP_RANKS};
    !has_any_rank(ctx.interface, t, RIP_RANKS)
}

fn target_needs_rake(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    use crate::engine::aura_helpers::{has_any_rank, RAKE_RANKS};
    !has_any_rank(ctx.interface, t, RAKE_RANKS)
}
