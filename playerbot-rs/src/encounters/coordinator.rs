/// Encounter coordinator — maps zone/area IDs to encounter FSMs.
///
/// When the bot enters a known instanced zone, the coordinator creates the
/// appropriate FSM.  The FSM is stored in `BotState::encounter`.
use super::EncounterFsm;

// ── Zone IDs (used to detect which instance the bot is in) ────────────────

pub const ZONE_MOLTEN_CORE: u32 = 2717;
pub const ZONE_ONYXIAS_LAIR: u32 = 2159;
pub const ZONE_BLACKWING_LAIR: u32 = 2677;
pub const ZONE_AQ20: u32 = 3429; // Ruins of Ahn'Qiraj
pub const ZONE_AQ40: u32 = 3428; // Temple of Ahn'Qiraj
pub const ZONE_NAXXRAMAS: u32 = 3456;
#[cfg(any(feature = "tbc", feature = "wotlk"))]
pub const ZONE_KARAZHAN: u32 = 3457;

/// Create a new encounter FSM for the given zone, if the zone is a known raid.
/// Returns `None` if the zone is not a known encounter zone.
///
/// The returned FSM starts in "pre-pull" state and becomes active when
/// `EncounterEvent::CombatStarted` is processed.
pub fn encounter_for_zone(zone_id: u32) -> Option<Box<dyn EncounterFsm>> {
    use crate::encounters::aq20::Aq20Fsm;
    use crate::encounters::aq40::Aq40Fsm;
    use crate::encounters::blackwing_lair::BlackwingLairFsm;
    use crate::encounters::molten_core::MoltenCoreFsm;
    use crate::encounters::naxxramas::NaxxramasFsm;
    use crate::encounters::onyxias_lair::OnyxiaFsm;

    match zone_id {
        ZONE_MOLTEN_CORE => Some(Box::new(MoltenCoreFsm::new())),
        ZONE_ONYXIAS_LAIR => Some(Box::new(OnyxiaFsm::new())),
        ZONE_BLACKWING_LAIR => Some(Box::new(BlackwingLairFsm::new())),
        ZONE_AQ20 => Some(Box::new(Aq20Fsm::new())),
        ZONE_AQ40 => Some(Box::new(Aq40Fsm::new())),
        ZONE_NAXXRAMAS => Some(Box::new(NaxxramasFsm::new())),
        #[cfg(any(feature = "tbc", feature = "wotlk"))]
        ZONE_KARAZHAN => Some(Box::new(crate::encounters::karazhan::KarazhanFsm::new())),
        _ => None,
    }
}
