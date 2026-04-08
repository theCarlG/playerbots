use crate::{Sel, Seq};
/// Restoration Shaman behavior tree (Classic / Vanilla).
///
/// Priority: Mana Spring Totem → Nature's Swiftness panic → critical heals →
///   fast heals → Chain Heal when multiple hurt → sustained heals → Purge
use crate::{
    data::spells::vanilla::shaman::*,
    engine::bt::Bt::{self, DropConfiguredTotems, GroupMembersBelow, CastOnSelf, HealLowest, HealInjuredParty, InCombat, CastOnTarget},
    engine::macro_fsm::ActiveFsm,
};

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (shaman-wide list).
        super::boost(),
        // Totem upkeep driven by per-bot preferences
        // (see `bot::class_prefs::ShamanPrefs`).
        Bt::throttle(2_000, DropConfiguredTotems),
        // Nature's Swiftness panic window for instant critical heal.
        Seq!(
            GroupMembersBelow(1, 0.20),
            Bt::self_missing(NATURE_SWIFTNESS),
            CastOnSelf(NATURE_SWIFTNESS),
        ),
        // Critical heals.
        HealLowest(HEALING_WAVE, 0.30),
        HealInjuredParty(HEALING_WAVE, 0.30),
        // Fast heals.
        HealLowest(LESSER_HEALING_WAVE, 0.55),
        HealInjuredParty(LESSER_HEALING_WAVE, 0.55),
        // Chain Heal when multiple hurt.
        Seq!(
            GroupMembersBelow(2, 0.75),
            HealInjuredParty(CHAIN_HEAL, 0.75),
        ),
        // Sustained.
        HealLowest(HEALING_WAVE, 0.80),
        HealInjuredParty(HEALING_WAVE, 0.80),
        // Purge enemy buffs.
        Seq!(InCombat, CastOnTarget(PURGE)),
    )
}
