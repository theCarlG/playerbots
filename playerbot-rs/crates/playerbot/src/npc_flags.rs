//! `UNIT_NPC_FLAG_*` bit values.
//!
//! These bits were **renumbered in TBC (2.0)**: vanilla uses a compact layout,
//! TBC/WotLK the modern expanded one (with vendor/trainer sub-types). The Rust
//! AI talks to the core through `get_nearby_npcs(range, flags)`, which ANDs
//! these against the creature's live `UNIT_NPC_FLAGS`, so a value that doesn't
//! match the built expansion silently scans the WRONG creatures — e.g. a
//! vanilla "vendor" scan using the `WotLK` `0x80` actually matches innkeepers,
//! and "repair" `0x1000` matches auctioneers. That left vendor/repair errands
//! dead on the classic build.
//!
//! Vanilla values verified against the Karatefylla classic fork's
//! `Entities/Unit.h`; TBC/`WotLK` against the canonical `CMaNGOS` layout.

#[cfg(feature = "vanilla")]
mod values {
    pub const GOSSIP: u32 = 0x0000_0001;
    pub const QUESTGIVER: u32 = 0x0000_0002;
    pub const VENDOR: u32 = 0x0000_0004;
    pub const FLIGHTMASTER: u32 = 0x0000_0008;
    pub const TRAINER: u32 = 0x0000_0010;
    pub const INNKEEPER: u32 = 0x0000_0080;
    pub const BANKER: u32 = 0x0000_0100;
    pub const AUCTIONEER: u32 = 0x0000_1000;
    pub const STABLEMASTER: u32 = 0x0000_2000;
    pub const REPAIR: u32 = 0x0000_4000;
}

#[cfg(any(feature = "tbc", feature = "wotlk"))]
mod values {
    pub const GOSSIP: u32 = 0x0000_0001;
    pub const QUESTGIVER: u32 = 0x0000_0002;
    pub const TRAINER: u32 = 0x0000_0010;
    pub const VENDOR: u32 = 0x0000_0080;
    pub const REPAIR: u32 = 0x0000_1000;
    pub const FLIGHTMASTER: u32 = 0x0000_2000;
    pub const INNKEEPER: u32 = 0x0001_0000;
    pub const BANKER: u32 = 0x0002_0000;
    pub const AUCTIONEER: u32 = 0x0020_0000;
    pub const STABLEMASTER: u32 = 0x0040_0000;
}

pub use values::*;

/// NPCs a bot will stroll over to "visit" while idling in a town — the set the
/// old `PossibleRpgTargetsValue` accepted. Having ANY of these bits makes a
/// creature an interesting errand/RP destination.
pub const RPG_VISIT_MASK: u32 = GOSSIP
    | QUESTGIVER
    | VENDOR
    | FLIGHTMASTER
    | TRAINER
    | INNKEEPER
    | BANKER
    | AUCTIONEER
    | STABLEMASTER
    | REPAIR;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_values_match_core_layout() {
        // QUESTGIVER never moved across expansions.
        assert_eq!(QUESTGIVER, 0x2);

        // Vanilla packs the bits tightly; the WotLK constants would have
        // matched the WRONG creatures here (0x80 = innkeeper, 0x1000 =
        // auctioneer), which is exactly the bug this module fixes.
        #[cfg(feature = "vanilla")]
        {
            assert_eq!(VENDOR, 0x4);
            assert_eq!(REPAIR, 0x4000);
            assert_eq!(INNKEEPER, 0x80);
            assert_eq!(TRAINER, 0x10);
        }
        #[cfg(any(feature = "tbc", feature = "wotlk"))]
        {
            assert_eq!(VENDOR, 0x80);
            assert_eq!(REPAIR, 0x1000);
            assert_eq!(TRAINER, 0x10);
        }

        // The visit mask must contain the core errand NPCs.
        assert_eq!(RPG_VISIT_MASK & VENDOR, VENDOR);
        assert_eq!(RPG_VISIT_MASK & REPAIR, REPAIR);
        assert_eq!(RPG_VISIT_MASK & QUESTGIVER, QUESTGIVER);
    }
}
