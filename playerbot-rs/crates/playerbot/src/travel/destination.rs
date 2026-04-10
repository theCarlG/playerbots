/// Travel destinations — typed targets the bot can travel toward.
///
/// Port of PB2's `TravelDestination`, `TravelDestinationPurpose`,
/// `TravelState`, and `TravelStatus` enumerations and core logic.
use cmangos::SpellId;

// ── TravelDestinationPurpose (PB2: TravelValues.h:35–58) ────────────────
// Bitmask enum — a single NPC/GO can serve multiple purposes.

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TravelPurpose: u32 {
        const NONE              = 0;
        const QUEST_GIVER       = 1 << 0;
        const QUEST_OBJECTIVE1  = 1 << 1;
        const QUEST_OBJECTIVE2  = 1 << 2;
        const QUEST_OBJECTIVE3  = 1 << 3;
        const QUEST_OBJECTIVE4  = 1 << 4;
        const QUEST_ALL_OBJ     = Self::QUEST_OBJECTIVE1.bits()
                                | Self::QUEST_OBJECTIVE2.bits()
                                | Self::QUEST_OBJECTIVE3.bits()
                                | Self::QUEST_OBJECTIVE4.bits();
        const QUEST_TAKER       = 1 << 5;
        const GENERIC_RPG       = 1 << 6;
        const TRAINER           = 1 << 7;
        const REPAIR            = 1 << 8;
        const VENDOR            = 1 << 9;
        const AH                = 1 << 10;
        const MAIL              = 1 << 11;
        const GRIND             = 1 << 12;
        const BOSS              = 1 << 13;
        const GATHER_SKINNING   = 1 << 14;
        const GATHER_MINING     = 1 << 15;
        const GATHER_HERBALISM  = 1 << 16;
        const GATHER_FISHING    = 1 << 17;
        const EXPLORE           = 1 << 18;
    }
}

impl TravelPurpose {
    pub fn short_name(self) -> &'static str {
        if self.contains(Self::QUEST_GIVER) {
            "questgiver"
        } else if self.intersects(Self::QUEST_ALL_OBJ) {
            "questobjective"
        } else if self.contains(Self::QUEST_TAKER) {
            "questtaker"
        } else if self.contains(Self::VENDOR) {
            "vendor"
        } else if self.contains(Self::AH) {
            "ah"
        } else if self.contains(Self::REPAIR) {
            "repair"
        } else if self.contains(Self::MAIL) {
            "mail"
        } else if self.contains(Self::TRAINER) {
            "trainer"
        } else if self.contains(Self::EXPLORE) {
            "explore"
        } else if self.contains(Self::GENERIC_RPG) {
            "rpg"
        } else if self.contains(Self::GRIND) {
            "grind"
        } else if self.contains(Self::BOSS) {
            "boss"
        } else if self.intersects(
            Self::GATHER_FISHING
                | Self::GATHER_HERBALISM
                | Self::GATHER_MINING
                | Self::GATHER_SKINNING,
        ) {
            "gather"
        } else {
            "unknown"
        }
    }
}

// ── TravelState (PB2: TravelMgr.h:295–307) ─────────────────────────────

/// High-level goal the bot's travel system is pursuing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum TravelState {
    #[default]
    Idle = 0,
    TravelPickUpQuest = 1,
    WorkPickUpQuest = 2,
    TravelDoQuest = 3,
    WorkDoQuest = 4,
    TravelHandInQuest = 5,
    WorkHandInQuest = 6,
    TravelRpg = 7,
    TravelExplore = 8,
}

// ── TravelStatus (PB2: TravelMgr.h:309–319) ────────────────────────────

/// Per-target lifecycle phase.
///
/// ```text
/// NONE → PREPARE → READY → TRAVEL → WORK → COOLDOWN → EXPIRED
///                                 ↑         │
///                                 └─────────┘ (retry)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum TravelStatus {
    #[default]
    None = 0,
    Prepare = 1,
    Ready = 2,
    Travel = 3,
    Work = 4,
    Cooldown = 5,
    Expired = 6,
}

// ── TravelDestination ───────────────────────────────────────────────────

/// A travel destination with coordinates, purpose metadata, and an
/// optional entry id (creature/GO that the bot should interact with).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TravelDestination {
    pub kind: TravelKind,
    pub purpose: TravelPurpose,
    pub map: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Creature or GO entry (positive = creature, negative = GO). 0 = none.
    pub entry: i32,
    /// Quest id this destination is associated with (0 = none).
    pub quest_id: u32,
}

/// What kind of travel this is — affects priority and completion logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TravelKind {
    /// Null destination — no real target.
    Null,
    /// Travel to a named location (city, inn, hub).
    NamedLocation,
    /// Travel to a quest objective.
    QuestObjective,
    /// Travel to a quest giver / taker.
    QuestRelation,
    /// Travel to a grind spot.
    GrindSpot,
    /// Travel to a world buff NPC/location.
    WorldBuff(SpellId),
    /// Travel to a vendor/repair NPC.
    Vendor,
    /// Travel to a flight master.
    FlightMaster,
    /// Travel to the bot's bind/home point.
    Home,
    /// Travel to an RPG target.
    Rpg,
    /// Travel to an explore zone.
    Explore,
    /// Travel to a boss.
    Boss,
    /// Travel to a gathering node.
    Gather,
    /// Travel to a mailbox.
    Mail,
    /// Travel to an AH NPC.
    AuctionHouse,
    /// Travel to a trainer.
    Trainer,
}

impl TravelDestination {
    pub fn new(kind: TravelKind, purpose: TravelPurpose, map: u32, x: f32, y: f32, z: f32) -> Self {
        Self {
            kind,
            purpose,
            map,
            x,
            y,
            z,
            entry: 0,
            quest_id: 0,
        }
    }

    pub fn with_entry(mut self, entry: i32) -> Self {
        self.entry = entry;
        self
    }

    pub fn with_quest(mut self, quest_id: u32) -> Self {
        self.quest_id = quest_id;
        self
    }

    /// Null destination that is always "arrived at".
    pub fn null() -> Self {
        Self {
            kind: TravelKind::Null,
            purpose: TravelPurpose::NONE,
            map: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            entry: 0,
            quest_id: 0,
        }
    }

    pub fn is_null(&self) -> bool {
        self.kind == TravelKind::Null
    }

    /// Squared distance to a point (same-map only, ignores Z).
    pub fn dist_sq_2d(&self, x: f32, y: f32) -> f32 {
        (self.x - x).powi(2) + (self.y - y).powi(2)
    }

    /// 3D distance to a point.
    pub fn distance_3d(&self, x: f32, y: f32, z: f32) -> f32 {
        ((self.x - x).powi(2) + (self.y - y).powi(2) + (self.z - z).powi(2)).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dist_sq_2d_calculates_correctly() {
        let dest = TravelDestination::new(
            TravelKind::NamedLocation,
            TravelPurpose::NONE,
            0,
            10.0,
            20.0,
            30.0,
        );
        let dist = dest.dist_sq_2d(13.0, 24.0);
        // 3^2 + 4^2 = 25
        assert!((dist - 25.0).abs() < 0.001);
    }

    #[test]
    fn purpose_short_names() {
        assert_eq!(TravelPurpose::VENDOR.short_name(), "vendor");
        assert_eq!(TravelPurpose::QUEST_GIVER.short_name(), "questgiver");
        assert_eq!(
            TravelPurpose::QUEST_OBJECTIVE1.short_name(),
            "questobjective"
        );
        assert_eq!(TravelPurpose::GATHER_FISHING.short_name(), "gather");
        assert_eq!(TravelPurpose::GRIND.short_name(), "grind");
    }

    #[test]
    fn null_destination() {
        let null = TravelDestination::null();
        assert!(null.is_null());
    }
}
