//! Class-agnostic per-bot preferences for instance-wide, cross-boss
//! behaviors (BWL suppression disarm, MC rune dousing, …).
//!
//! Instance FSMs read these via `ctx.settings.encounter_prefs` inside
//! their `zone_wide_bt()` handler fns. The default is "any eligible bot
//! participates" — a raid leader can whisper `suppression forbid` /
//! `douse force` to override per bot.

/// Opt-in / opt-out control for a cross-boss duty.
///
/// * `Auto` — default. Fall through to the instance-specific eligibility
///   rule (e.g. class == Rogue for suppression, holds quintessence for
///   dousing).
/// * `Forbid` — never participate, even if eligible.
/// * `Force` — always participate when in the gated area, regardless of
///   the normal eligibility check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DutyMode {
    #[default]
    Auto,
    Forbid,
    Force,
}

impl DutyMode {
    /// Parser helper for the `suppression` / `douse` chat commands.
    pub fn from_word(s: &str) -> Option<Self> {
        Some(match s {
            "auto" => Self::Auto,
            "forbid" | "off" | "no" => Self::Forbid,
            "force" | "on" | "yes" => Self::Force,
            _ => return None,
        })
    }

    pub fn as_word(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Forbid => "forbid",
            Self::Force => "force",
        }
    }
}

/// Encounter / instance-wide behavior preferences. Sibling of
/// `ClassPrefs` on `BotSettings` — class-agnostic because the relevant
/// participants (quintessence carriers, off-class disarmers) are not
/// pinned to any single class.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EncounterPrefs {
    /// BWL — rogues disarm Suppression Devices in the pre-Broodlord
    /// corridor. `Auto` = any rogue in the area participates.
    pub suppression_duty: DutyMode,
    /// MC — designated carriers douse the runes before Majordomo.
    /// `Auto` = any bot currently holding Aqual/Eternal Quintessence
    /// participates.
    pub douse_duty: DutyMode,
}
