//! Player spec name/id lookups — Rust port of
//! `RandomItemMgr::GetPlayerSpecName` / `GetPlayerSpecId` plus the
//! inline `GetPlayerSpecTab` helper (RandomItemMgr.cpp 15-35, 2600-2714).
//!
//! `GetPlayerSpecTab` — the talent-point dominance scan over `sTalentStore` —
//! is not reimplemented here because the talent DBC only lives on the
//! C++ side. Instead, [`super::manager::ItemPoolManager`] consults
//! [`cmangos::ItemWorld::player_talent_tab`] which returns the best tab
//! already computed by the bridge, and feeds that into
//! [`player_spec_name`].
//!
//! Two small randomized forks survive from the legacy code:
//!
//! * Druid tab 1: level > 19 picks between `feraltank` and `feraldps`
//!   via a 50/50 roll.
//! * Death knight tab 1 (wotlk only): picks between `frosttank` and
//!   `frostdps` via a 50/50 roll.
//!
//! The randomness is injected as an `FnMut(u32, u32) -> u32` closure so
//! the function stays testable without pulling in the full `ItemWorld`
//! trait. Callers in the production path pass `|min, max|
//! world.urand_range(min, max)`.

use super::equip_filter::class;
use super::types::WeightScale;

/// Resolve the spec name for a `(class, best_tab, level)` tuple.
///
/// Returns `None` for classes that do not appear in the legacy C++
/// switch (e.g. death knight on classic/tbc, or an unknown class id).
///
/// `urand` must behave like the `CMaNGOS` `urand(min, max)` helper:
/// inclusive on both ends, uniformly distributed. Only consulted for
/// druid feral (level > 19, tab 1) and wotlk death-knight frost (tab 1).
#[must_use]
pub fn player_spec_name<R>(
    class_id: u8,
    tab: u32,
    level: u32,
    mut urand: R,
) -> Option<&'static str>
where
    R: FnMut(u32, u32) -> u32,
{
    match class_id {
        class::PRIEST => Some(match tab {
            2 => "shadow",
            1 => "holy",
            _ => "disc",
        }),
        class::SHAMAN => Some(match tab {
            2 => "resto",
            1 => "enhance",
            _ => "elem",
        }),
        class::WARRIOR => Some(match tab {
            2 => "prot",
            1 => "fury",
            _ => "arms",
        }),
        class::PALADIN => match tab {
            0 => Some("holy"),
            1 => Some("prot"),
            2 => Some("retrib"),
            _ => None,
        },
        class::DRUID => match tab {
            0 => Some("balance"),
            1 => {
                // `level > 19 && urand(0, 100) > 50` picks the dps side.
                if level > 19 && urand(0, 100) > 50 {
                    Some("feraldps")
                } else {
                    Some("feraltank")
                }
            }
            2 => Some("resto"),
            _ => None,
        },
        class::ROGUE => match tab {
            0 => Some("assas"),
            1 => Some("combat"),
            2 => Some("subtle"),
            _ => None,
        },
        class::HUNTER => match tab {
            0 => Some("beast"),
            1 => Some("marks"),
            2 => Some("surv"),
            _ => None,
        },
        #[cfg(feature = "wotlk")]
        class::DEATH_KNIGHT => match tab {
            0 => Some("blooddps"),
            1 => {
                // DK frost has no level gate in the legacy code.
                if urand(0, 100) > 50 {
                    Some("frostdps")
                } else {
                    Some("frosttank")
                }
            }
            2 => Some("unholydps"),
            _ => None,
        },
        class::MAGE => match tab {
            0 => Some("arcane"),
            1 => Some("fire"),
            2 => Some("frost"),
            _ => None,
        },
        class::WARLOCK => match tab {
            0 => Some("afflic"),
            1 => Some("demo"),
            2 => Some("destro"),
            _ => None,
        },
        _ => None,
    }
}

/// Map a spec name back to its `WeightScaleInfo::id` using the loaded
/// scale table. Returns `0` when the name is not present for the given
/// class (matching the C++ `GetPlayerSpecId` return-zero fallback).
///
/// `scales` should be the full slice of scales the manager loaded from
/// `ai_playerbot_weight_scales`; the function performs a linear scan
/// because the table is tiny (≤ 32 rows) and only hit during cache
/// refresh.
#[must_use]
pub fn player_spec_id(scales: &[WeightScale], class_id: u8, spec_name: &str) -> u32 {
    if spec_name.is_empty() {
        return 0;
    }
    for scale in scales {
        if scale.info.class_id == class_id && scale.info.name == spec_name {
            return scale.info.id;
        }
    }
    0
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itempool::types::{WeightScale, WeightScaleInfo};

    /// urand stub that always returns `min` — the druid/DK branches
    /// then pick the tank side (`>` 50 is false when urand returns 0).
    fn urand_min(min: u32, _max: u32) -> u32 {
        min
    }

    /// urand stub that always returns `max` — picks the dps side.
    fn urand_max(_min: u32, max: u32) -> u32 {
        max
    }

    #[test]
    fn warrior_tab_to_spec_name() {
        assert_eq!(
            player_spec_name(class::WARRIOR, 0, 60, urand_min),
            Some("arms")
        );
        assert_eq!(
            player_spec_name(class::WARRIOR, 1, 60, urand_min),
            Some("fury")
        );
        assert_eq!(
            player_spec_name(class::WARRIOR, 2, 60, urand_min),
            Some("prot")
        );
    }

    #[test]
    fn priest_tab_to_spec_name() {
        // Priest is the class with the "else -> disc" default branch —
        // any tab that isn't 1 or 2 should become disc.
        assert_eq!(
            player_spec_name(class::PRIEST, 0, 60, urand_min),
            Some("disc")
        );
        assert_eq!(
            player_spec_name(class::PRIEST, 1, 60, urand_min),
            Some("holy")
        );
        assert_eq!(
            player_spec_name(class::PRIEST, 2, 60, urand_min),
            Some("shadow")
        );
    }

    #[test]
    fn paladin_tab_to_spec_name() {
        assert_eq!(
            player_spec_name(class::PALADIN, 0, 60, urand_min),
            Some("holy")
        );
        assert_eq!(
            player_spec_name(class::PALADIN, 1, 60, urand_min),
            Some("prot")
        );
        assert_eq!(
            player_spec_name(class::PALADIN, 2, 60, urand_min),
            Some("retrib")
        );
    }

    #[test]
    fn druid_feral_split_low_level_stays_tank() {
        // Level ≤ 19 short-circuits the dps branch regardless of roll.
        assert_eq!(
            player_spec_name(class::DRUID, 1, 19, urand_max),
            Some("feraltank")
        );
        // Level > 19 + "max roll" picks dps.
        assert_eq!(
            player_spec_name(class::DRUID, 1, 20, urand_max),
            Some("feraldps")
        );
        // Level > 19 + "min roll" keeps tank.
        assert_eq!(
            player_spec_name(class::DRUID, 1, 60, urand_min),
            Some("feraltank")
        );
    }

    #[test]
    fn druid_non_feral_tabs() {
        assert_eq!(
            player_spec_name(class::DRUID, 0, 60, urand_min),
            Some("balance")
        );
        assert_eq!(
            player_spec_name(class::DRUID, 2, 60, urand_min),
            Some("resto")
        );
    }

    #[cfg(feature = "wotlk")]
    #[test]
    fn dk_frost_split() {
        assert_eq!(
            player_spec_name(class::DEATH_KNIGHT, 0, 80, urand_min),
            Some("blooddps")
        );
        assert_eq!(
            player_spec_name(class::DEATH_KNIGHT, 1, 80, urand_min),
            Some("frosttank")
        );
        assert_eq!(
            player_spec_name(class::DEATH_KNIGHT, 1, 80, urand_max),
            Some("frostdps")
        );
        assert_eq!(
            player_spec_name(class::DEATH_KNIGHT, 2, 80, urand_min),
            Some("unholydps")
        );
    }

    #[cfg(not(feature = "wotlk"))]
    #[test]
    fn dk_not_available_pre_wotlk() {
        // Class id 6 is reserved on Classic/TBC; any call should bail
        // out with None because the match arm is cfg-gated away.
        assert_eq!(player_spec_name(6, 0, 60, urand_min), None);
    }

    #[test]
    fn unknown_class_returns_none() {
        assert_eq!(player_spec_name(99, 0, 60, urand_min), None);
        assert_eq!(player_spec_name(0, 0, 60, urand_min), None);
    }

    fn make_scale(id: u32, name: &str, class_id: u8) -> WeightScale {
        WeightScale {
            info: WeightScaleInfo {
                id,
                name: name.to_string(),
                class_id,
            },
            stats: Vec::new(),
        }
    }

    #[test]
    fn spec_id_lookup_matches_by_class_and_name() {
        let scales = vec![
            make_scale(1, "arms", class::WARRIOR),
            make_scale(2, "fury", class::WARRIOR),
            make_scale(3, "prot", class::WARRIOR),
            make_scale(4, "holy", class::PALADIN),
            make_scale(5, "holy", class::PRIEST),
        ];

        assert_eq!(player_spec_id(&scales, class::WARRIOR, "arms"), 1);
        assert_eq!(player_spec_id(&scales, class::WARRIOR, "prot"), 3);

        // "holy" exists for both paladin and priest — class_id must
        // disambiguate so we pick the right row.
        assert_eq!(player_spec_id(&scales, class::PALADIN, "holy"), 4);
        assert_eq!(player_spec_id(&scales, class::PRIEST, "holy"), 5);
    }

    #[test]
    fn spec_id_empty_name_returns_zero() {
        let scales = vec![make_scale(1, "arms", class::WARRIOR)];
        assert_eq!(player_spec_id(&scales, class::WARRIOR, ""), 0);
    }

    #[test]
    fn spec_id_unknown_name_returns_zero() {
        let scales = vec![make_scale(1, "arms", class::WARRIOR)];
        assert_eq!(player_spec_id(&scales, class::WARRIOR, "ghost"), 0);
        // Known name but wrong class also returns zero.
        assert_eq!(player_spec_id(&scales, class::PALADIN, "arms"), 0);
    }
}
