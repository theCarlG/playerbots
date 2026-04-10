/// AI Level-of-Detail — scales processing depth with human proximity.
///
/// Bots near human players get full BDI/GOAP/BT processing. Bots far
/// from humans get simplified behavior at reduced tick rates. This
/// keeps 1,800+ bots affordable while ensuring grouped bots feel alive.
use crate::bdi::desires::DesireKind;
use crate::bot::state::BotState;

/// AI processing depth tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AiLod {
    /// Grouped with human, in combat near human, or in instance with human.
    /// Full BDI evaluation (500ms), GOAP on intention change, full BT every tick.
    #[default]
    Full = 0,
    /// Solo but a human player is nearby (~100yd). Life-like behavior,
    /// BDI every 2s, full BT every tick.
    Active = 1,
    /// Solo, no humans nearby, but bot has active goals (grinding, traveling).
    /// BDI every 5s, simplified BT, 500ms tick rate.
    Background = 2,
    /// Far from all humans and idle. Near-zero CPU.
    /// No BDI, hold intention, minimal BT, 2000ms tick rate.
    Dormant = 3,
}

impl AiLod {
    /// How often BDI desire evaluation should run at this LOD tier (ms).
    pub fn bdi_interval_ms(self) -> u64 {
        match self {
            Self::Full => 500,
            Self::Active => 2_000,
            Self::Background => 5_000,
            Self::Dormant => u64::MAX, // never — hold current intention
        }
    }

    /// Whether the bot should skip this tick entirely at this LOD tier.
    /// Uses the bot handle as a stable hash for staggering.
    pub fn should_skip_tick(self, bot_handle: u64, server_time_ms: u64) -> bool {
        match self {
            Self::Full | Self::Active => false,
            Self::Background => {
                // Tick every ~500ms (skip 4 out of 5 ticks)
                let slot = bot_handle % 5;
                let tick_slot = (server_time_ms / 100) % 5;
                slot != tick_slot
            }
            Self::Dormant => {
                // Tick every ~2000ms (skip 19 out of 20 ticks)
                let slot = bot_handle % 20;
                let tick_slot = (server_time_ms / 100) % 20;
                slot != tick_slot
            }
        }
    }

    /// Whether reactive subtrees (interrupt, dispel, etc.) should run.
    pub fn run_reactive_subtrees(self) -> bool {
        matches!(self, Self::Full | Self::Active)
    }

    /// Whether the nearby-unit scan should run.
    pub fn should_scan_nearby(self) -> bool {
        matches!(self, Self::Full | Self::Active)
    }
}

/// Determine the LOD tier for a bot based on its current state.
///
/// Called at the start of each tick, before any processing.
pub fn determine_lod(bot: &BotState) -> AiLod {
    let has_human_master = bot.master_guid.is_some();
    let in_instance = !bot.snap.is_overworld;

    // Full LOD: grouped with a human or in an instance
    if has_human_master || in_instance {
        return AiLod::Full;
    }

    // Check if any nearby unit is likely a human player.
    // A non-bot player has npc_entry == 0 (it's a player character) and
    // we don't manage it (it's not in our bot pool). Since we can't
    // cheaply query the bot pool from here, we use a heuristic: any
    // player in the group with a master (master_guid != 0 on the group
    // member) is managed, but the master itself is human.
    //
    // Fallback: if the bot is in a group, it's likely near a human.
    // Solo bots without a master or group default to Background/Dormant.
    let has_human_nearby = bot.snap.group_size > 0;

    if has_human_nearby {
        return AiLod::Active;
    }

    // Background if the bot has an active intention
    if bot.bdi.active_desire() != DesireKind::Idle {
        return AiLod::Background;
    }

    AiLod::Dormant
}
