/// Pet management — summon, revive, feed (Hunter/Warlock).
use crate::engine::bt::Bt::{self, *};
use crate::{Seq, Sel};

pub fn pet_subtree() -> Bt {
    Seq!(
        IsPetClass,
        Sel!(
            // Revive dead pet.
            Seq!(HasPet, PetAlive.not(), InCombat.not(), RevivePet),
            // Summon pet if none.
            Seq!(HasPet.not(), InCombat.not(), SummonPet),
            // Feed unhappy pet (Hunter only).
            Seq!(
                IsClass(crate::bot::state::PlayerClass::Hunter),
                HasPet,
                PetAlive,
                PetUnhappy,
                InCombat.not(),
                Bt::throttle(30_000, FeedPet),
            ),
        ),
    )
}
