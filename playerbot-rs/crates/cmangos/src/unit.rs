#![forbid(unsafe_code)]

use crate::BotUnitSnapshot;

/// Borrowed reference to a unit snapshot. Zero-cost wrapper around
/// `&BotUnitSnapshot` carrying a lifetime so the borrow checker can track
/// references handed out from `World::get_unit_snapshot`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct UnitRef<'a>(pub &'a BotUnitSnapshot);

impl<'a> UnitRef<'a> {
    #[inline]
    pub fn snapshot(self) -> &'a BotUnitSnapshot {
        self.0
    }
}
