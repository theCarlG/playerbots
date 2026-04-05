/// Declarative macro that generates an `EncounterFsm` impl for an enum whose
/// variants are homogeneous newtypes wrapping inner FSMs.
///
/// Usage:
/// ```ignore
/// encounter_dispatch! {
///     #[derive(Clone, PartialEq)]
///     pub enum MoltenCoreBoss {
///         Lucifron(LucifronFsm),
///         Magmadar(MagmadarFsm),
///         // ...
///     }
/// }
/// ```
///
/// Expands to the enum definition plus an `impl EncounterFsm for <name>` that
/// forwards every trait method to the inner FSM via match-and-forward.
macro_rules! encounter_dispatch {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $variant:ident ( $inner:ty ) ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $( $variant($inner), )+
        }

        impl $crate::encounters::EncounterFsm for $name {
            fn update(
                &mut self,
                event: &$crate::encounters::EncounterEvent,
                boss_hp_pct: f32,
                time_ms: u64,
            ) {
                match self {
                    $( Self::$variant(fsm) => fsm.update(event, boss_hp_pct, time_ms), )+
                }
            }
            fn phase_id(&self) -> u32 {
                match self { $( Self::$variant(fsm) => fsm.phase_id(), )+ }
            }
            fn is_active(&self) -> bool {
                match self { $( Self::$variant(fsm) => fsm.is_active(), )+ }
            }
            fn is_done(&self) -> bool {
                match self { $( Self::$variant(fsm) => fsm.is_done(), )+ }
            }
            fn boss_entry(&self) -> u32 {
                match self { $( Self::$variant(fsm) => fsm.boss_entry(), )+ }
            }
            fn phase_bt(&self) -> Option<$crate::engine::bt::Bt> {
                match self { $( Self::$variant(fsm) => fsm.phase_bt(), )+ }
            }
            fn safe_zone_hint(&self) -> u8 {
                match self { $( Self::$variant(fsm) => fsm.safe_zone_hint(), )+ }
            }
        }
    };
}

pub(crate) use encounter_dispatch;
