//! Zero-cost newtype wrappers for type safety across the FFI boundary.
//!
//! These prevent accidentally mixing spell IDs with item IDs, zone IDs, etc.
//! All are `#[repr(transparent)]` so they have identical layout to the inner type.

#![forbid(unsafe_code)]

/// Generate a zero-cost newtype ID wrapper with standard derives, `NONE`
/// sentinel, `raw()` accessor, bidirectional `From<u32>` conversions, and
/// a `Display` impl. The macro is `#[repr(transparent)]` so the generated
/// type is ABI-compatible with `u32` across FFI.
macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
        #[repr(transparent)]
        pub struct $name(pub u32);

        impl $name {
            pub const NONE: $name = $name(0);

            /// Raw u32 value — use at the FFI boundary only.
            #[inline]
            pub const fn raw(self) -> u32 {
                self.0
            }
        }

        impl From<u32> for $name {
            #[inline]
            fn from(v: u32) -> Self {
                Self(v)
            }
        }

        impl From<$name> for u32 {
            #[inline]
            fn from(v: $name) -> Self {
                v.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

define_id!(/// A `WoW` spell ID (e.g. Mortal Strike = 21553).
SpellId);

define_id!(/// A `WoW` item ID (e.g. Hearthstone = 6948).
ItemId);

define_id!(/// A `WoW` skill ID.
SkillId);

define_id!(/// A `WoW` talent ID.
TalentId);

/// Role bitmask — matches the `role` field in `BotUnitSnapshot`.
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
