/// Zero-cost newtype wrappers for type safety across the FFI boundary.
///
/// These prevent accidentally mixing spell IDs with item IDs, zone IDs, etc.
/// All are `#[repr(transparent)]` so they have identical layout to the inner type.

/// A WoW spell ID (e.g. Mortal Strike = 21553).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
#[repr(transparent)]
pub struct SpellId(pub u32);

impl SpellId {
    pub const NONE: SpellId = SpellId(0);

    /// Raw u32 value — use at the FFI boundary only.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl From<u32> for SpellId {
    #[inline]
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<SpellId> for u32 {
    #[inline]
    fn from(v: SpellId) -> Self {
        v.0
    }
}

/// A WoW item ID (e.g. Hearthstone = 6948).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ItemId(pub u32);

impl ItemId {
    pub const NONE: ItemId = ItemId(0);

    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl From<u32> for ItemId {
    #[inline]
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<ItemId> for u32 {
    #[inline]
    fn from(v: ItemId) -> Self {
        v.0
    }
}

/// Role bitmask — matches the `role` field in BotUnitSnapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BotRole(pub u8);

impl BotRole {
    pub const NONE: BotRole = BotRole(0);
    pub const TANK: BotRole = BotRole(1);
    pub const HEAL: BotRole = BotRole(2);
    pub const DPS: BotRole = BotRole(4);

    pub fn is_tank(self) -> bool {
        self.0 & 1 != 0
    }
    pub fn is_heal(self) -> bool {
        self.0 & 2 != 0
    }
    pub fn is_dps(self) -> bool {
        self.0 & 4 != 0
    }
}

impl std::fmt::Display for SpellId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SpellId({})", self.0)
    }
}

impl std::fmt::Display for ItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ItemId({})", self.0)
    }
}
