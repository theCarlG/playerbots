/// Pet management — summon, revive, feed (Hunter/Warlock).
use crate::engine::bt::Bt::{self, *};

pub fn pet_subtree() -> Bt {
    Seq(vec![
        IsPetClass,
        Sel(vec![
            // Revive dead pet.
            Seq(vec![HasPet, PetAlive.not(), InCombat.not(), RevivePet]),
            // Summon pet if none.
            Seq(vec![HasPet.not(), InCombat.not(), SummonPet]),
            // Feed unhappy pet (Hunter only).
            Seq(vec![
                IsClass(crate::bot::state::PlayerClass::Hunter),
                HasPet,
                PetAlive,
                PetUnhappy,
                InCombat.not(),
                Bt::throttle(30_000, FeedPet),
            ]),
        ]),
    ])
}
