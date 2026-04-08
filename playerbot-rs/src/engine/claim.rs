//! Claim table for group-wide coordination.
//!
//! Prevents duplicate work (double-heals, two bots dousing the same rune,
//! multiple bots picking up the same add). Each claim is a (claimant, data)
//! pair with a TTL — claims auto-expire so a dead bot doesn't permanently
//! block a resource.
//!
//! Lives inside `GroupState` (shared via `Arc<RwLock<>>`). Bots call
//! `try_claim()` before acting and `is_claimed()` to check.

use crate::ffi::UnitHandle;

/// Maximum number of concurrent claims per group. Fixed-size array — no heap
/// allocation, predictable cache behavior for 40-man raids.
const MAX_CLAIMS: usize = 32;

/// What is being claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimData {
    /// A heal target — only one healer should commit a cast to this target.
    HealTarget(UnitHandle),
    /// A CC target — only one bot should crowd-control this mob.
    CcTarget(UnitHandle),
    /// A rune to douse (MC) or similar interactable objective.
    DouseRune(u32),
    /// An add to pick up (OT duty).
    AddPickup(UnitHandle),
    /// A tank-swap target — the bot claiming this will taunt next.
    TankSwapTarget(UnitHandle),
    /// Generic claim with a u32 kind tag and u64 payload.
    Custom(u32, u64),
}

impl ClaimData {
    /// Returns the "subject" of the claim for matching purposes.
    /// Two claims with the same subject conflict — only one can be active.
    fn subject(&self) -> (u8, u64) {
        match *self {
            ClaimData::HealTarget(h) => (0, h),
            ClaimData::CcTarget(h) => (1, h),
            ClaimData::DouseRune(id) => (2, id as u64),
            ClaimData::AddPickup(h) => (3, h),
            ClaimData::TankSwapTarget(h) => (4, h),
            ClaimData::Custom(kind, payload) => (5 + kind as u8, payload),
        }
    }
}

/// A single claim entry.
#[derive(Debug, Clone, Copy)]
pub struct Claim {
    /// Who owns this claim.
    pub claimant: UnitHandle,
    /// Server time (ms) when the claim was created.
    pub claimed_at_ms: u64,
    /// How long the claim lives before auto-expiring (ms).
    pub ttl_ms: u64,
    /// What is being claimed.
    pub data: ClaimData,
}

impl Claim {
    fn is_expired(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.claimed_at_ms) >= self.ttl_ms
    }
}

/// Fixed-size claim table shared across all bots in a group.
#[derive(Debug, Default)]
pub struct ClaimTable {
    claims: [Option<Claim>; MAX_CLAIMS],
}

impl ClaimTable {
    pub fn new() -> Self {
        Self {
            claims: [const { None }; MAX_CLAIMS],
        }
    }

    /// Try to claim a resource. Returns `true` if the claim was placed.
    ///
    /// Fails if the same subject is already claimed by another bot (and not
    /// expired). Succeeds if:
    /// - The subject is unclaimed
    /// - The existing claim is expired
    /// - The claimant already owns this claim (refreshes TTL)
    pub fn try_claim(
        &mut self,
        claimant: UnitHandle,
        data: ClaimData,
        ttl_ms: u64,
        now_ms: u64,
    ) -> bool {
        let subject = data.subject();

        // Check for existing claim on the same subject.
        for slot in self.claims.iter_mut() {
            if let Some(c) = slot {
                if c.data.subject() == subject {
                    if c.claimant == claimant {
                        // Refresh our own claim.
                        c.claimed_at_ms = now_ms;
                        c.ttl_ms = ttl_ms;
                        return true;
                    }
                    if c.is_expired(now_ms) {
                        // Expired — take it over.
                        *c = Claim {
                            claimant,
                            claimed_at_ms: now_ms,
                            ttl_ms,
                            data,
                        };
                        return true;
                    }
                    // Active claim by someone else — deny.
                    return false;
                }
            }
        }

        // No existing claim — find a free slot.
        for slot in self.claims.iter_mut() {
            if slot.is_none() {
                *slot = Some(Claim {
                    claimant,
                    claimed_at_ms: now_ms,
                    ttl_ms,
                    data,
                });
                return true;
            }
        }

        // Table full — try to evict an expired claim.
        for slot in self.claims.iter_mut() {
            if let Some(c) = slot {
                if c.is_expired(now_ms) {
                    *slot = Some(Claim {
                        claimant,
                        claimed_at_ms: now_ms,
                        ttl_ms,
                        data,
                    });
                    return true;
                }
            }
        }

        false // Table full, no expired slots.
    }

    /// Check if a subject is claimed by someone other than `me`.
    pub fn is_claimed_by_other(&self, me: UnitHandle, data: &ClaimData, now_ms: u64) -> bool {
        let subject = data.subject();
        self.claims.iter().any(|slot| {
            slot.as_ref().is_some_and(|c| {
                c.data.subject() == subject && c.claimant != me && !c.is_expired(now_ms)
            })
        })
    }

    /// Check if a subject is claimed (by anyone, including self).
    pub fn is_claimed(&self, data: &ClaimData, now_ms: u64) -> bool {
        let subject = data.subject();
        self.claims.iter().any(|slot| {
            slot.as_ref()
                .is_some_and(|c| c.data.subject() == subject && !c.is_expired(now_ms))
        })
    }

    /// Release all claims held by `claimant`.
    pub fn release_all(&mut self, claimant: UnitHandle) {
        for slot in self.claims.iter_mut() {
            if let Some(c) = slot {
                if c.claimant == claimant {
                    *slot = None;
                }
            }
        }
    }

    /// Release a specific claim held by `claimant`.
    pub fn release(&mut self, claimant: UnitHandle, data: &ClaimData) {
        let subject = data.subject();
        for slot in self.claims.iter_mut() {
            if let Some(c) = slot {
                if c.claimant == claimant && c.data.subject() == subject {
                    *slot = None;
                    return;
                }
            }
        }
    }

    /// Garbage-collect expired claims.
    pub fn gc(&mut self, now_ms: u64) {
        for slot in self.claims.iter_mut() {
            if let Some(c) = slot {
                if c.is_expired(now_ms) {
                    *slot = None;
                }
            }
        }
    }

    /// Number of active (non-expired) claims.
    pub fn active_count(&self, now_ms: u64) -> usize {
        self.claims
            .iter()
            .filter(|s| s.as_ref().is_some_and(|c| !c.is_expired(now_ms)))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOT_A: UnitHandle = 0xAAAA;
    const BOT_B: UnitHandle = 0xBBBB;
    const MOB_1: UnitHandle = 0x1111;
    const MOB_2: UnitHandle = 0x2222;

    #[test]
    fn basic_claim_and_check() {
        let mut table = ClaimTable::new();
        let data = ClaimData::HealTarget(MOB_1);
        assert!(table.try_claim(BOT_A, data, 5000, 1000));
        assert!(table.is_claimed(&data, 1000));
        assert!(table.is_claimed_by_other(BOT_B, &data, 1000));
        assert!(!table.is_claimed_by_other(BOT_A, &data, 1000));
    }

    #[test]
    fn duplicate_claim_denied() {
        let mut table = ClaimTable::new();
        let data = ClaimData::HealTarget(MOB_1);
        assert!(table.try_claim(BOT_A, data, 5000, 1000));
        assert!(!table.try_claim(BOT_B, data, 5000, 1000));
    }

    #[test]
    fn same_claimant_refreshes() {
        let mut table = ClaimTable::new();
        let data = ClaimData::HealTarget(MOB_1);
        assert!(table.try_claim(BOT_A, data, 5000, 1000));
        assert!(table.try_claim(BOT_A, data, 5000, 3000)); // refresh
        // Should still be active at 7999 (3000 + 5000 - 1)
        assert!(table.is_claimed(&data, 7999));
        // Expired at 8000
        assert!(!table.is_claimed(&data, 8000));
    }

    #[test]
    fn expired_claim_can_be_taken_over() {
        let mut table = ClaimTable::new();
        let data = ClaimData::HealTarget(MOB_1);
        assert!(table.try_claim(BOT_A, data, 2000, 1000));
        // At time 3000, claim is expired (1000 + 2000)
        assert!(table.try_claim(BOT_B, data, 5000, 3000));
        assert!(table.is_claimed_by_other(BOT_A, &data, 3000));
    }

    #[test]
    fn different_subjects_independent() {
        let mut table = ClaimTable::new();
        let heal = ClaimData::HealTarget(MOB_1);
        let cc = ClaimData::CcTarget(MOB_1);
        assert!(table.try_claim(BOT_A, heal, 5000, 1000));
        assert!(table.try_claim(BOT_B, cc, 5000, 1000)); // different type, same target
    }

    #[test]
    fn release_specific_claim() {
        let mut table = ClaimTable::new();
        let data = ClaimData::HealTarget(MOB_1);
        assert!(table.try_claim(BOT_A, data, 5000, 1000));
        table.release(BOT_A, &data);
        assert!(!table.is_claimed(&data, 1000));
        // Now another bot can take it.
        assert!(table.try_claim(BOT_B, data, 5000, 1000));
    }

    #[test]
    fn release_all_clears_only_claimant() {
        let mut table = ClaimTable::new();
        let h1 = ClaimData::HealTarget(MOB_1);
        let h2 = ClaimData::HealTarget(MOB_2);
        assert!(table.try_claim(BOT_A, h1, 5000, 1000));
        assert!(table.try_claim(BOT_B, h2, 5000, 1000));
        table.release_all(BOT_A);
        assert!(!table.is_claimed(&h1, 1000));
        assert!(table.is_claimed(&h2, 1000)); // BOT_B still holds this
    }

    #[test]
    fn gc_removes_expired() {
        let mut table = ClaimTable::new();
        let data = ClaimData::DouseRune(42);
        assert!(table.try_claim(BOT_A, data, 1000, 0));
        assert_eq!(table.active_count(500), 1);
        assert_eq!(table.active_count(1000), 0);
        table.gc(1000);
        // Slot is now free.
        assert!(table.try_claim(BOT_B, data, 5000, 1000));
    }

    #[test]
    fn table_full_evicts_expired() {
        let mut table = ClaimTable::new();
        // Fill all 32 slots with expired claims.
        for i in 0..32u64 {
            assert!(table.try_claim(BOT_A, ClaimData::Custom(0, i), 100, 0));
        }
        // All expired at time 200.
        // New claim should evict one.
        assert!(table.try_claim(BOT_B, ClaimData::Custom(1, 0), 5000, 200));
    }
}
