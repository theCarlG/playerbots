/// Group buffing — apply persistent buffs to party/raid members out of combat.
///
/// Each spec registers the buffs it can apply.  The module scans all group
/// members (and self) and issues a cast for the first member missing a buff.
/// One buff is applied per tick to avoid cast spam.
///
/// Throttled at 5 s so we don't poll every tick.  In combat the subtree
/// returns Failure immediately so it never interrupts the rotation.
use crate::engine::bt_nodes::{BtNode, BtResult, action, cond, seq, throttle};
use crate::ffi::SpellId;

/// Who can receive this buff.
#[derive(Debug, Clone, Copy)]
pub enum BuffTarget {
    /// Cast on self only.
    Me,
    /// Cast on any group member (including self) missing the aura.
    AnyMember,
    /// Cast specifically on the group's designated tank.
    Tank,
    /// Cast specifically on the group's designated healer.
    Healer,
}

/// One buff the bot knows how to apply.
#[derive(Debug, Clone)]
pub struct GroupBuff {
    /// Spell ID to cast (highest rank — C++ auto-downranks).
    pub spell_id: SpellId,
    /// All ranks of the aura to check. The buff is considered present if ANY
    /// rank is found. This prevents infinite rebuffing when a lower-level bot
    /// applies a lower rank (aura ID differs from the cast spell ID).
    pub aura_ranks: &'static [SpellId],
    /// Who should receive the buff.
    pub target: BuffTarget,
}

impl GroupBuff {
    pub const fn on_self(spell_id: SpellId, aura_ranks: &'static [SpellId]) -> Self {
        Self {
            spell_id,
            aura_ranks,
            target: BuffTarget::Me,
        }
    }

    pub const fn on_party(spell_id: SpellId, aura_ranks: &'static [SpellId]) -> Self {
        Self {
            spell_id,
            aura_ranks,
            target: BuffTarget::AnyMember,
        }
    }

    pub const fn on_party_aura(
        spell_id: SpellId,
        aura_ranks: &'static [SpellId],
    ) -> Self {
        Self {
            spell_id,
            aura_ranks,
            target: BuffTarget::AnyMember,
        }
    }
}

/// Build a buff subtree for the given buff list.
///
/// Returns Failure when in combat.  Returns Success when a buff cast is issued.
/// Returns Failure (and falls through to follow/idle) when all buffs are present.
pub fn build_buff_subtree(buffs: Vec<GroupBuff>) -> Box<dyn BtNode> {
    seq(vec![
        // Only buff out of combat.
        cond(|ctx| !ctx.in_combat()),
        throttle(
            5_000,
            action(move |ctx| {
                for buff in &buffs {
                    if let Some(target_handle) = find_buff_target(ctx, buff)
                        && ctx.interface.cast_spell(buff.spell_id, target_handle)
                    {
                        ctx.timers.on_spell_cast(buff.spell_id, ctx.server_time_ms);
                        return BtResult::Success;
                    }
                }
                BtResult::Failure
            }),
        ),
    ])
}

/// Find the first unit that needs this buff.  Returns None when all eligible
/// targets already have the aura (any rank).
fn find_buff_target(
    ctx: &mut crate::engine::context::TickContext<'_>,
    buff: &GroupBuff,
) -> Option<u64> {
    use crate::engine::aura_helpers::has_any_rank;
    use crate::ffi::UnitHandle;
    use BuffTarget::{AnyMember, Healer, Me, Tank};

    let me = ctx.bot_handle;

    match buff.target {
        Me => {
            if !has_any_rank(ctx.interface, me, buff.aura_ranks) {
                Some(me)
            } else {
                None
            }
        }

        Tank => ctx
            .interface
            .group_get_tank()
            .filter(|&t| !has_any_rank(ctx.interface, t, buff.aura_ranks)),

        Healer => ctx
            .interface
            .group_get_healer()
            .filter(|&h| !has_any_rank(ctx.interface, h, buff.aura_ranks)),

        AnyMember => {
            // Check self first, then all group members.
            let candidates: Vec<UnitHandle> = std::iter::once(me)
                .chain(
                    ctx.snap.group_members[..ctx.snap.group_size as usize]
                        .iter()
                        .copied()
                        .filter(|&h| h != 0 && h != me),
                )
                .collect();

            candidates
                .into_iter()
                .find(|&h| !has_any_rank(ctx.interface, h, buff.aura_ranks))
        }
    }
}
