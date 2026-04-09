/// Personality — per-bot weights that bias desire selection.
///
/// Each dimension is 0.0 to 1.0. Default is 0.5 (balanced). Personality
/// multiplies raw urgency scores in `desires::score_desires()`.
///
/// Configurable at runtime via chat command: `personality aggression 0.8`.

/// Per-bot personality profile.
#[derive(Debug, Clone, Copy)]
pub struct Personality {
    /// Weights `KillTarget`, `PullMobs`, `GrindMobs`.
    /// 0.0 = passive, 1.0 = berserker.
    pub aggression: f32,
    /// Weights `Survive`, `FleeFromDanger`.
    /// 0.0 = reckless, 1.0 = cowardly.
    pub caution: f32,
    /// Weights `HealGroup`, `ResurrectDead`, `DispelDebuffs`, `ProtectAlly`, `BuffGroup`.
    /// 0.0 = selfish, 1.0 = selfless.
    pub helpfulness: f32,
    /// Scales intention `min_hold_ms` duration.
    /// 0.0 = chaotic (switches fast), 1.0 = rigid (sticks to plan).
    pub discipline: f32,
    /// Weights `PullMobs`, `CoordinateGroup`.
    /// 0.0 = passive follower, 1.0 = proactive leader.
    pub initiative: f32,
}

impl Default for Personality {
    fn default() -> Self {
        Self {
            aggression: 0.5,
            caution: 0.5,
            helpfulness: 0.5,
            discipline: 0.5,
            initiative: 0.5,
        }
    }
}

impl Personality {
    /// Parse a personality dimension name from a chat-command token.
    /// Returns `None` if the name is not recognized.
    pub fn set_dimension(&mut self, name: &str, value: f32) -> bool {
        let value = value.clamp(0.0, 1.0);
        match name {
            "aggression" | "aggro" | "agg" => {
                self.aggression = value;
                true
            }
            "caution" | "caut" => {
                self.caution = value;
                true
            }
            "helpfulness" | "help" => {
                self.helpfulness = value;
                true
            }
            "discipline" | "disc" => {
                self.discipline = value;
                true
            }
            "initiative" | "init" => {
                self.initiative = value;
                true
            }
            _ => false,
        }
    }

    /// Role-appropriate default personality.
    pub fn default_for_role(role: crate::ffi::BotRole) -> Self {
        if role.is_tank() {
            Self {
                aggression: 0.4,
                caution: 0.3,
                helpfulness: 0.6,
                discipline: 0.8,
                initiative: 0.7,
            }
        } else if role.is_heal() {
            Self {
                aggression: 0.2,
                caution: 0.6,
                helpfulness: 0.9,
                discipline: 0.7,
                initiative: 0.3,
            }
        } else {
            // DPS
            Self {
                aggression: 0.7,
                caution: 0.4,
                helpfulness: 0.4,
                discipline: 0.5,
                initiative: 0.4,
            }
        }
    }
}
