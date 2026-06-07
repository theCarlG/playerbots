/// Travel planner — selects the best travel destination for a bot.
///
/// PB2's `TravelMgr` evaluates all possible destinations (quest objectives,
/// grind spots, world buffs, vendors) and picks the highest-priority one
/// the bot can reach. This module provides the Rust equivalent that works
/// with the blackboard-based travel system and the `TravelTarget` FSM.
use crate::engine::blackboard::{Blackboard, Key, Value};
use crate::travel::destination::{TravelDestination, TravelPurpose};
use crate::travel::graph;

/// Write a travel destination into the blackboard so `TravelToBlackboard`
/// can navigate to it.
pub fn set_travel_dest(bb: &mut Blackboard, dest: &TravelDestination) {
    bb.set(Key::TravelDestX, Value::F32(dest.x));
    bb.set(Key::TravelDestY, Value::F32(dest.y));
    bb.set(Key::TravelDestZ, Value::F32(dest.z));
    bb.set(Key::TravelDestMap, Value::U32(dest.map));
}

/// Clear the current travel destination from the blackboard.
pub fn clear_travel_dest(bb: &mut Blackboard) {
    bb.clear(Key::TravelDestX);
    bb.clear(Key::TravelDestY);
    bb.clear(Key::TravelDestZ);
    bb.clear(Key::TravelDestMap);
}

/// Check if a travel destination is currently set.
pub fn has_travel_dest(bb: &Blackboard) -> bool {
    bb.get_f32(Key::TravelDestX).is_some()
}

/// Pick the best destination from a list of candidates, filtering by
/// reachability (same map) and sorting by distance.
pub fn pick_nearest_reachable(
    current_map: u32,
    from_x: f32,
    from_y: f32,
    candidates: &[TravelDestination],
) -> Option<TravelDestination> {
    candidates
        .iter()
        .filter(|d| graph::is_reachable(current_map, d))
        .min_by(|a, b| {
            let da = a.dist_sq_2d(from_x, from_y);
            let db = b.dist_sq_2d(from_x, from_y);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

/// Evaluate what the bot needs and return a prioritized list of purposes.
///
/// Port of PB2's `TravelStrategy::InitNonCombatTriggers` priority table.
/// Returns (purpose, relevance) sorted by relevance descending.
#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
pub fn evaluate_needs(
    durability_pct: f32,
    bags_full: bool,
    has_sellable: bool,
    free_quest_slots: u8,
    has_complete_quests: bool,
    has_incomplete_quests: bool,
    town_errand: bool,
    _level: u8,
) -> Vec<(TravelPurpose, f32)> {
    let mut needs = Vec::with_capacity(8);

    // Questing is the PRIMARY activity. A maintenance trip is triggered only when
    // it's genuinely needed (gear failing, bags full, or a quest ready to hand
    // in). But ONCE a town trip is underway (`town_errand`), the bot batches ALL
    // town chores — turn in, sell, repair — before heading back out, so a visit
    // feels intentional instead of one-errand-then-leave. The `town_errand` flag
    // is set by the chooser when it picks any town destination and stays set
    // (refreshed) until there are no town chores left, at which point questing
    // resumes.

    // Repair — when gear is nearly broken (always), OR opportunistically while
    // already on a town visit if it's taken meaningful wear.
    if durability_pct < 0.25 {
        needs.push((TravelPurpose::REPAIR, 6.95));
    } else if town_errand && durability_pct < 0.85 {
        needs.push((TravelPurpose::REPAIR, 6.846));
    }

    // Vendor — when bags are full (always), OR opportunistically while on a town
    // visit if there's anything worth selling ("might as well sell while here").
    if bags_full {
        needs.push((TravelPurpose::VENDOR, 6.94));
    } else if town_errand && has_sellable {
        needs.push((TravelPurpose::VENDOR, 6.848));
    }

    // Quest taker — turn in completed quests first (frees a slot + rewards).
    if has_complete_quests {
        needs.push((TravelPurpose::QUEST_TAKER, 6.85));
    }

    // Quest objective — route to where the incomplete quests' required mobs
    // are (resolved via FindCreatureData on the C++ side), instead of
    // grinding wherever the bot happens to be standing. Ranked ABOVE the quest
    // giver: do the quests we already hold before collecting more. Otherwise a
    // bot with free log slots keeps walking back to the (now exhausted) giver
    // and never works its objective — it just oscillates.
    if has_incomplete_quests {
        needs.push((TravelPurpose::QUEST_ALL_OBJ, 6.84));
    }

    // Quest giver — when quest log has free slots (and nothing more pressing).
    if free_quest_slots > 0 {
        needs.push((TravelPurpose::QUEST_GIVER, 6.83));
    }

    // Grind — always available as fallback.
    needs.push((TravelPurpose::GRIND, 6.27));

    // Explore — low priority fallback.
    needs.push((TravelPurpose::EXPLORE, 6.29));

    // RPG — idle wandering.
    #[allow(clippy::approx_constant)]
    {
        needs.push((TravelPurpose::GENERIC_RPG, 6.28));
    }

    // Sort by relevance descending.
    needs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    needs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::travel::destination::{TravelDestination, TravelKind};

    #[test]
    fn set_and_check_travel_dest() {
        let mut bb = Blackboard::default();
        assert!(!has_travel_dest(&bb));

        let dest = TravelDestination::new(
            TravelKind::NamedLocation,
            TravelPurpose::NONE,
            0,
            100.0,
            200.0,
            300.0,
        );
        set_travel_dest(&mut bb, &dest);
        assert!(has_travel_dest(&bb));
        assert_eq!(bb.get_f32(Key::TravelDestX), Some(100.0));

        clear_travel_dest(&mut bb);
        assert!(!has_travel_dest(&bb));
    }

    #[test]
    fn pick_nearest_filters_by_map() {
        let candidates = vec![
            TravelDestination::new(
                TravelKind::NamedLocation,
                TravelPurpose::NONE,
                1,
                10.0,
                10.0,
                0.0,
            ), // wrong map
            TravelDestination::new(
                TravelKind::NamedLocation,
                TravelPurpose::NONE,
                0,
                100.0,
                0.0,
                0.0,
            ), // far
            TravelDestination::new(
                TravelKind::NamedLocation,
                TravelPurpose::NONE,
                0,
                10.0,
                0.0,
                0.0,
            ), // near
        ];
        let best = pick_nearest_reachable(0, 0.0, 0.0, &candidates);
        assert!(best.is_some());
        assert!((best.unwrap().x - 10.0).abs() < 0.001);
    }

    #[test]
    fn pick_nearest_returns_none_when_no_reachable() {
        let candidates = vec![TravelDestination::new(
            TravelKind::NamedLocation,
            TravelPurpose::NONE,
            1,
            10.0,
            10.0,
            0.0,
        )];
        assert!(pick_nearest_reachable(0, 0.0, 0.0, &candidates).is_none());
    }

    #[test]
    fn evaluate_needs_prioritizes_repair() {
        let needs = evaluate_needs(0.1, false, false, 3, false, false, false, 30);
        // Repair should be first (highest relevance when durability is low).
        assert_eq!(needs[0].0, TravelPurpose::REPAIR);
    }

    #[test]
    fn evaluate_needs_always_includes_grind() {
        let needs = evaluate_needs(1.0, false, false, 0, false, false, false, 60);
        assert!(needs.iter().any(|(p, _)| *p == TravelPurpose::GRIND));
    }

    #[test]
    fn evaluate_needs_routes_to_objectives_for_incomplete_quests() {
        let needs = evaluate_needs(1.0, false, false, 0, false, true, false, 30);
        assert!(
            needs.iter().any(|(p, _)| *p == TravelPurpose::QUEST_ALL_OBJ),
            "incomplete quests should emit a quest-objective travel need"
        );
    }
}
