/// Push events from C++ — aura changes, spell casts, unit deaths, damage taken.
///
/// These are queued by the `playerbot_*` push-event exports and processed at
/// the start of the next tick before the BT runs.
use crate::ffi::{SpellId, UnitHandle};

#[derive(Debug, Clone)]
pub enum BotEvent {
    /// A unit visible to this bot cast (or failed to cast) a spell.
    UnitSpellCast {
        caster: UnitHandle,
        spell_id: SpellId,
        target: UnitHandle,
        success: bool,
    },
    /// An aura was applied to or removed from a unit visible to this bot.
    AuraChanged {
        unit: UnitHandle,
        spell_id: SpellId,
        applied: bool,
        stacks: u8,
    },
    /// A unit visible to this bot died.
    UnitDied {
        victim: UnitHandle,
        killer: UnitHandle,
    },
    /// This bot took damage.
    DamageTaken {
        damage: u32,
        spell_id: SpellId, // SpellId::NONE = melee
        dealer: UnitHandle,
    },
    /// A raw network packet (opcode + raw bytes), for packet-based triggers.
    PacketIn {
        opcode: u16,
        data: Vec<u8>,
    },
    PacketOut {
        opcode: u16,
        data: Vec<u8>,
    },
}
