use std::sync::OnceLock;

/// Shazzrah encounter — Molten Core.
///
/// Single-phase caster fight. Key mechanics:
///   - **Shazzrah's Curse** (19714): reduced magic resists on raid — cleanse.
///   - **Arcane Explosion** (19712): ~10y PBAOE — melee stay away when channel.
///   - **Counterspell**: Shazzrah silences casters — rotate casts.
///   - **Gate of Shazzrah** (23138): teleports to random raid member.
///     Everyone MUST spread out so a teleport doesn't chain-kill stacked bots.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, IsMeleeDps, IsRanged, MaintainRange, Sel, Seq};
use crate::ffi::SpellId;

pub const AURA_SHAZZRAH_CURSE: SpellId = SpellId(19714);
pub const SPELL_ARCANE_EXPLOSION: SpellId = SpellId(19712);
pub const SPELL_GATE_OF_SHAZZRAH: SpellId = SpellId(23138);

static BT: OnceLock<Bt> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
pub struct ShazzrahFsm {
    active: bool,
    done: bool,
}

impl PartialEq for ShazzrahFsm {
    fn eq(&self, other: &Self) -> bool {
        self.active == other.active && self.done == other.done
    }
}

impl ShazzrahFsm {
    pub fn new() -> Self {
        Self {
            active: false,
            done: false,
        }
    }

    fn static_bt() -> &'static Bt {
        BT.get_or_init(|| {
            Sel(vec![
                Seq(vec![IsRanged, MaintainRange(30.0)]),
                Seq(vec![IsMeleeDps, MaintainRange(5.0)]),
            ])
        })
    }
}

impl Default for ShazzrahFsm {
    fn default() -> Self {
        Self::new()
    }
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
    fn phase_bt(&self) -> Option<&Bt> {
        if self.active {
            Some(Self::static_bt())
        } else {
            None
        }
    }
}
