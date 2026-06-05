/// Shazzrah encounter — Molten Core.
///
/// Single-phase caster fight. Key mechanics:
///   - **Shazzrah's Curse** (19714): reduced magic resists on raid — cleanse.
///   - **Arcane Explosion** (19712): ~10y PBAOE — melee stay away when channel.
///   - **Counterspell**: Shazzrah silences casters — rotate casts.
///   - **Gate of Shazzrah** (23138): teleports to random raid member.
///     Everyone spreads so the post-teleport Arcane Explosion doesn't chain a
///     stacked group, and the tank re-taunts the instant the boss is loose on
///     someone else (Gate wipes his threat table).
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, IsMeleeDps, IsRanged, IsTank, MaintainRange, Taunt};
use crate::engine::bt::BehaviorLeaf;
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use crate::{Sel, Seq};
use cmangos::SpellId;

pub const AURA_SHAZZRAH_CURSE: SpellId = SpellId(19714);
pub const SPELL_ARCANE_EXPLOSION: SpellId = SpellId(19712);
pub const SPELL_GATE_OF_SHAZZRAH: SpellId = SpellId(23138);

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ShazzrahFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for ShazzrahFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { .. } => self.done = true,
            EncounterEvent::GroupWipe => self.active = false,
            _ => {}
        }
    }
    fn phase_id(&self) -> u32 {
        u32::from(self.active)
    }
    fn is_active(&self) -> bool {
        self.active
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        super::ENTRY_SHAZZRAH
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(
                // Gate wiped threat — the tank re-taunts to drag Shazzrah back
                // off whichever raid member he teleported to. Falls through
                // (taunt on cooldown / boss already on the tank) to the
                // tank's normal threat-building rotation.
                Seq!(IsTank, Bt::Custom(BOSS_OFF_TANK), Taunt),
                Seq!(IsRanged, MaintainRange(30.0)),
                Seq!(IsMeleeDps, MaintainRange(5.0)),
            ))
        } else {
            None
        }
    }
}

/// True when the boss (the tank's current target) is attacking someone other
/// than this tank — i.e. Gate of Shazzrah just wiped threat and teleported him
/// onto another raid member, so the tank should taunt him back.
const BOSS_OFF_TANK: BehaviorLeaf = BehaviorLeaf {
    label: "shazzrah_boss_off_tank",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        let Some(boss) = ctx.current_target() else {
            return BtResult::Failure;
        };
        let victim = ctx.interface.get_unit_snapshot(boss).current_target;
        if victim != 0 && victim != ctx.bot_handle {
            BtResult::Success
        } else {
            BtResult::Failure
        }
    },
    display_text: None,
};
