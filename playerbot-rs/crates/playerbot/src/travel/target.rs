/// Travel target — per-bot travel target with lifecycle state machine.
///
/// Port of PB2's `TravelTarget` class (`TravelMgr.h:328–400`).
///
/// The lifecycle:
/// ```text
/// NONE → PREPARE (wait, no loot nearby) → READY → TRAVEL (move toward dest)
///   → WORK (grind/rpg at dest) → COOLDOWN (free time) → EXPIRED (pick new)
/// ```
use crate::travel::destination::{TravelDestination, TravelStatus};

/// Default times (ms) from PB2.
const DEFAULT_EXPIRE_DELAY: u32 = 300_000; // 5 minutes
const DEFAULT_COOLDOWN_DELAY: u32 = 60_000; // 1 minute
const PREPARE_TIMEOUT: u32 = 10_000; // 10 seconds max in PREPARE
const WORK_TIMEOUT: u32 = 60_000; // 1 minute max in WORK
#[allow(dead_code)]
const COOLDOWN_TIMEOUT: u32 = 60_000; // 1 minute max in COOLDOWN

/// Per-bot travel target tracking.
#[derive(Debug)]
pub struct TravelTarget {
    pub destination: TravelDestination,
    pub status: TravelStatus,
    /// Server time (ms) when the current status started.
    pub start_time_ms: u64,
    /// How long the current status is allowed to last (ms).
    pub status_timeout_ms: u32,
    /// How many times we've retried movement.
    pub move_retry_count: u32,
    /// How many times we've extended the WORK phase.
    pub extend_retry_count: u32,
    /// True if this target was set by a forced command (not auto-selected).
    pub forced: bool,
    /// Priority/relevance score — higher = more important.
    pub relevance: u32,
}

impl Default for TravelTarget {
    fn default() -> Self {
        Self {
            destination: TravelDestination::null(),
            status: TravelStatus::None,
            start_time_ms: 0,
            status_timeout_ms: 0,
            move_retry_count: 0,
            extend_retry_count: 0,
            forced: false,
            relevance: 0,
        }
    }
}

impl TravelTarget {
    /// Set a new destination and begin the lifecycle.
    pub fn set(&mut self, dest: TravelDestination, now_ms: u64) {
        self.destination = dest;
        self.status = TravelStatus::Prepare;
        self.start_time_ms = now_ms;
        self.status_timeout_ms = PREPARE_TIMEOUT;
        self.move_retry_count = 0;
        self.extend_retry_count = 0;
        self.forced = false;
        self.relevance = 0;
    }

    /// Set a forced destination (from explicit command).
    pub fn set_forced(&mut self, dest: TravelDestination, now_ms: u64) {
        self.set(dest, now_ms);
        self.forced = true;
    }

    /// Clear the travel target — go to idle.
    pub fn clear(&mut self) {
        self.destination = TravelDestination::null();
        self.status = TravelStatus::None;
        self.move_retry_count = 0;
        self.extend_retry_count = 0;
        self.forced = false;
        self.relevance = 0;
    }

    /// Is the travel target active (has a real destination in a non-terminal status)?
    pub fn is_active(&self) -> bool {
        !self.destination.is_null()
            && !matches!(self.status, TravelStatus::None | TravelStatus::Expired)
    }

    /// Is the bot currently traveling toward the destination?
    pub fn is_traveling(&self) -> bool {
        self.status == TravelStatus::Travel
    }

    /// Is the bot working at the destination?
    pub fn is_working(&self) -> bool {
        self.status == TravelStatus::Work
    }

    /// Has this status timed out?
    pub fn is_timed_out(&self, now_ms: u64) -> bool {
        self.status_timeout_ms > 0
            && now_ms.saturating_sub(self.start_time_ms) >= u64::from(self.status_timeout_ms)
    }

    /// Advance status to the next phase.
    pub fn advance(&mut self, now_ms: u64) {
        let next = match self.status {
            TravelStatus::None => TravelStatus::Prepare,
            TravelStatus::Prepare => TravelStatus::Ready,
            TravelStatus::Ready => TravelStatus::Travel,
            TravelStatus::Travel => TravelStatus::Work,
            TravelStatus::Work => TravelStatus::Cooldown,
            TravelStatus::Cooldown => TravelStatus::Expired,
            TravelStatus::Expired => TravelStatus::Expired,
        };
        self.set_status(next, now_ms);
    }

    /// Explicitly set status with fresh timeout.
    pub fn set_status(&mut self, status: TravelStatus, now_ms: u64) {
        self.status = status;
        self.start_time_ms = now_ms;
        self.status_timeout_ms = match status {
            TravelStatus::None => 0,
            TravelStatus::Prepare => PREPARE_TIMEOUT,
            TravelStatus::Ready => 5_000,
            TravelStatus::Travel => DEFAULT_EXPIRE_DELAY,
            TravelStatus::Work => WORK_TIMEOUT,
            TravelStatus::Cooldown => DEFAULT_COOLDOWN_DELAY,
            TravelStatus::Expired => 0,
        };
    }

    /// Check status and auto-advance if timed out. Returns the current status.
    /// Port of PB2 `TravelTarget::CheckStatus`.
    pub fn check_status(&mut self, now_ms: u64, at_destination: bool) {
        if self.destination.is_null() {
            self.status = TravelStatus::None;
            return;
        }

        if self.is_timed_out(now_ms) {
            match self.status {
                TravelStatus::Prepare | TravelStatus::Ready => {
                    // Timed out waiting — go to travel.
                    self.set_status(TravelStatus::Travel, now_ms);
                }
                TravelStatus::Travel => {
                    if self.move_retry_count > 10 {
                        self.set_status(TravelStatus::Expired, now_ms);
                    } else {
                        // Reset timeout, increment retry.
                        self.move_retry_count += 2;
                        self.start_time_ms = now_ms;
                    }
                }
                TravelStatus::Work => {
                    self.set_status(TravelStatus::Cooldown, now_ms);
                }
                TravelStatus::Cooldown => {
                    self.set_status(TravelStatus::Expired, now_ms);
                }
                _ => {}
            }
        }

        // If we arrived at the destination while traveling, advance to work.
        if self.status == TravelStatus::Travel && at_destination {
            self.set_status(TravelStatus::Work, now_ms);
        }
    }

    /// Time elapsed since status started (ms).
    pub fn elapsed_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.start_time_ms)
    }

    /// Estimated travel time at run speed (distance / 7 yds/sec).
    pub fn estimate_travel_time_secs(&self, from_x: f32, from_y: f32) -> f32 {
        self.destination.dist_sq_2d(from_x, from_y).sqrt() / 7.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::travel::destination::{TravelKind, TravelPurpose};

    fn make_dest() -> TravelDestination {
        TravelDestination::new(
            TravelKind::Vendor,
            TravelPurpose::VENDOR,
            0,
            100.0,
            200.0,
            10.0,
        )
    }

    #[test]
    fn set_and_clear() {
        let mut tt = TravelTarget::default();
        assert!(!tt.is_active());

        tt.set(make_dest(), 1000);
        assert!(tt.is_active());
        assert_eq!(tt.status, TravelStatus::Prepare);

        tt.clear();
        assert!(!tt.is_active());
    }

    #[test]
    fn advance_lifecycle() {
        let mut tt = TravelTarget::default();
        tt.set(make_dest(), 0);
        assert_eq!(tt.status, TravelStatus::Prepare);

        tt.advance(100);
        assert_eq!(tt.status, TravelStatus::Ready);

        tt.advance(200);
        assert_eq!(tt.status, TravelStatus::Travel);

        tt.advance(300);
        assert_eq!(tt.status, TravelStatus::Work);

        tt.advance(400);
        assert_eq!(tt.status, TravelStatus::Cooldown);

        tt.advance(500);
        assert_eq!(tt.status, TravelStatus::Expired);
    }

    #[test]
    fn check_status_timeout_in_prepare() {
        let mut tt = TravelTarget::default();
        tt.set(make_dest(), 0);
        assert_eq!(tt.status, TravelStatus::Prepare);

        // Not yet timed out.
        tt.check_status(5_000, false);
        assert_eq!(tt.status, TravelStatus::Prepare);

        // Timed out — should advance to Travel.
        tt.check_status(11_000, false);
        assert_eq!(tt.status, TravelStatus::Travel);
    }

    #[test]
    fn check_status_arrival() {
        let mut tt = TravelTarget::default();
        tt.set(make_dest(), 0);
        tt.set_status(TravelStatus::Travel, 0);

        // Arrived — should move to Work.
        tt.check_status(1_000, true);
        assert_eq!(tt.status, TravelStatus::Work);
    }

    #[test]
    fn travel_retry_then_expire() {
        let mut tt = TravelTarget::default();
        tt.set(make_dest(), 0);
        tt.set_status(TravelStatus::Travel, 0);

        let timeout = DEFAULT_EXPIRE_DELAY as u64;
        // Timeout repeatedly until max retry. Each iteration advances time
        // past the current status timeout.
        let mut now = 0u64;
        for i in 0..10 {
            now += timeout + 1;
            tt.check_status(now, false);
            if tt.status == TravelStatus::Expired {
                break;
            }
            assert_eq!(
                tt.status,
                TravelStatus::Travel,
                "Should still be traveling at retry {i}"
            );
        }
        assert_eq!(tt.status, TravelStatus::Expired);
    }
}
