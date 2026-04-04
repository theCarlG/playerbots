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
    /// Spell ID to cast.
    pub spell_id: SpellId,
    /// Aura ID to check (often == spell_id; use the buff's spell effect ID when different).
    pub aura_id: SpellId,
    /// Who should receive the buff.
    pub target: BuffTarget,
}

impl GroupBuff {
    pub const fn on_self(spell_id: SpellId) -> Self {
        Self {
            spell_id,
            aura_id: spell_id,
            target: BuffTarget::Me,
        }
    }

    pub const fn on_party(spell_id: SpellId) -> Self {
        Self {
            spell_id,
            aura_id: spell_id,
            target: BuffTarget::AnyMember,
        }
    }

    pub const fn on_party_aura(spell_id: SpellId, aura_id: SpellId) -> Self {
        Self {
            spell_id,
            aura_id,
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
                    if let Some(target_handle) = find_buff_target(ctx, buff) {
                        if ctx.interface.cast_spell(buff.spell_id, target_handle) {
                            ctx.timers.on_spell_cast(buff.spell_id, ctx.server_time_ms);
                            return BtResult::Success;
                        }
                    }
                }
                BtResult::Failure
            }),
        ),
    ])
}

/// Find the first unit that needs this buff.  Returns None when all eligible
/// targets already have the aura.
fn find_buff_target(
    ctx: &mut crate::engine::context::TickContext<'_>,
    buff: &GroupBuff,
) -> Option<u64> {
    use crate::ffi::UnitHandle;
    use BuffTarget::*;

    let me = ctx.bot_handle;

    match buff.target {
        Me => {
            if !ctx.interface.has_aura(me, buff.aura_id) {
                Some(me)
            } else {
                None
            }
        }

        Tank => ctx
            .interface
            .group_get_tank()
            .filter(|&t| !ctx.interface.has_aura(t, buff.aura_id)),

        Healer => ctx
            .interface
            .group_get_healer()
            .filter(|&h| !ctx.interface.has_aura(h, buff.aura_id)),

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
                .find(|&h| !ctx.interface.has_aura(h, buff.aura_id))
        }
    }
}
