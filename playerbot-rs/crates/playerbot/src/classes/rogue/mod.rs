pub mod assassination;
pub mod combat;
pub mod poisons;
pub mod subtlety;

use crate::{
    Sel, Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::rogue::{ADRENALINE_RUSH, BLADE_FLURRY, COLD_BLOOD, GARROTE, STEALTH},
    engine::{
        bt::Bt::{self, CastOnSelf, CastOnTarget, InCombat, StrategyEnabled},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[];

/// `co +boost` burst subtree — rogue offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        Sel!(
            CastOnSelf(ADRENALINE_RUSH),
            CastOnSelf(BLADE_FLURRY),
            CastOnSelf(COLD_BLOOD),
        ),
    )
}

/// Pre-combat stealth approach + opener, shared by every rogue spec.
///
/// While the `STEALTH` strategy is enabled and the bot is not yet in
/// combat, slip into Stealth and — once the close/behind positioning has
/// brought the bot in range behind the target — open with Garrote. The
/// instant opener fires before the first auto-swing (which only starts its
/// ~weapon-speed timer on engage), breaks stealth, and hands off to the
/// spec rotation on the following (now in-combat) tick.
fn stealth_approach() -> Bt {
    Seq!(
        InCombat.not(),
        StrategyEnabled(StrategyFlags::STEALTH),
        Sel!(
            // Not stealthed yet — slip into Stealth (can_cast gates OOC).
            Seq!(Bt::self_missing(STEALTH), CastOnSelf(STEALTH)),
            // Stealthed, behind the target and in melee range — open with
            // Garrote (can_cast gates on the stealth aura + facing + range).
            CastOnTarget(GARROTE),
        ),
    )
}

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{RogueAssassination, RogueCombat, RogueSubtlety};
    let spec_tree = match spec {
        RogueAssassination => assassination::build_tree(fsm),
        RogueCombat => combat::build_tree(fsm),
        RogueSubtlety => subtlety::build_tree(fsm),
        _ => unreachable!("non-rogue spec passed to rogue::build_tree"),
    };
    // The Combat FSM also covers the stealthed approach before the first
    // hit, so prepend the shared stealth opener ahead of the spec rotation;
    // it falls through (Failure) the moment the bot is in combat.
    match fsm {
        ActiveFsm::Combat => Sel!(stealth_approach(), spec_tree),
        _ => spec_tree,
    }
}

pub fn buffs(_spec: PlayerSpec) -> &'static [GroupBuff] {
    BUFFS
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` rogue branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::{RogueAssassination, RogueCombat, RogueSubtlety};
    use StrategyFlags as F;
    let common = F::DPS_ASSIST
        | F::AOE
        | F::CLOSE
        | F::CC
        | F::BEHIND
        | F::STEALTH
        | F::POISONS
        | F::BUFF
        | F::BOOST;
    match spec {
        RogueAssassination => F::ASSASSINATION | common,
        RogueCombat => F::ROGUE_COMBAT | common,
        RogueSubtlety => F::SUBTLETY | common,
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a rogue `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::{RogueAssassination, RogueCombat, RogueSubtlety};
    use StrategyFlags as F;
    if flags.contains(F::ASSASSINATION) {
        return Some(RogueAssassination);
    }
    if flags.contains(F::ROGUE_COMBAT) {
        return Some(RogueCombat);
    }
    if flags.contains(F::SUBTLETY) {
        return Some(RogueSubtlety);
    }
    None
}
