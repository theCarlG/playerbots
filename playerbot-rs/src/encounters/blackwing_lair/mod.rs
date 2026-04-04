/// Blackwing Lair — 8 boss encounters.
///
/// Zone ID: 2677.  40-player raid.
///
/// Bosses in order:
///   1. Razorgore the Untamed    (entry 12435)
///   2. Vaelastrasz the Corrupt  (entry 13020)
///   3. Broodlord Lashlayer      (entry 12017)
///   4. Firemaw                  (entry 11983)
///   5. Ebonroc                  (entry 14601)
///   6. Flamegor                 (entry 14610)
///   7. Chromaggus               (entry 14020)
///   8. Nefarian                 (entry 11583)

use super::{EncounterEvent, EncounterFsm, SimpleFsm};

pub const ENTRY_RAZORGORE:    u32 = 12435;
pub const ENTRY_VAELASTRASZ:  u32 = 13020;
pub const ENTRY_BROODLORD:    u32 = 12017;
pub const ENTRY_FIREMAW:      u32 = 11983;
pub const ENTRY_EBONROC:      u32 = 14601;
pub const ENTRY_FLAMEGOR:     u32 = 14610;
pub const ENTRY_CHROMAGGUS:   u32 = 14020;
pub const ENTRY_NEFARIAN:     u32 = 11583;

/// Nefarian mechanic — Class Calls (spell IDs for each class call effect).
/// The class call forces bots to use a specific action or causes negative effects.
pub mod nefarian_class_calls {
    pub const WARRIOR_CALL:   u32 = 23397; // Warriors must use Berserker Stance
    pub const DRUID_CALL:     u32 = 23398; // Druids shift to Bear form
    pub const PRIEST_CALL:    u32 = 23401; // Priests are silenced
    pub const PALADIN_CALL:   u32 = 23418; // Paladins unequip weapons
    pub const SHAMAN_CALL:    u32 = 23425; // Shaman totems pulse
    pub const MAGE_CALL:      u32 = 23410; // Mages polymorph each other
    pub const WARLOCK_CALL:   u32 = 23419; // Warlocks channel Power Word: Shield
    pub const HUNTER_CALL:    u32 = 23409; // Hunter pets become uncontrollable
    pub const ROGUE_CALL:     u32 = 23414; // Rogues are forced to Eviscerate
}

/// Vaelastrasz — Burning Adrenaline (aura 18173): forces a player to cast
/// Fire/Death spells before their HP hits 0.  Affects tanks most severely.
pub const AURA_BURNING_ADRENALINE: crate::ffi::SpellId = crate::ffi::SpellId(18173);

#[derive(Clone, Copy, PartialEq)]
enum ActiveBoss {
    None,
    Razorgore, Vaelastrasz, Broodlord,
    Firemaw, Ebonroc, Flamegor, Chromaggus,
    Nefarian,
}

/// Nefarian FSM — 3 phases.
///
/// Phase 1 (100%→22%): Ground phase. Class calls. 40 Drakonid adds throughout.
/// Phase 2 (<22%): Nefarian takes flight briefly, zombie drakonids spawn.
/// Phase 3: Nefarian lands again, fight continues until death.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NefPhase { Idle, Ground, Air, FinalGround }

struct NefFsm {
    phase: NefPhase,
    done:  bool,
}

impl NefFsm {
    fn new() -> Self { Self { phase: NefPhase::Idle, done: false } }
    fn update_inner(&mut self, event: &EncounterEvent, boss_hp: f32, _t: u64) {
        match event {
            EncounterEvent::CombatStarted => { self.phase = NefPhase::Ground; }
            EncounterEvent::UnitDied { .. } => { self.done = true; }
            EncounterEvent::GroupWipe => { self.phase = NefPhase::Idle; }
            EncounterEvent::None => {
                if self.phase == NefPhase::Ground && boss_hp < 0.22 {
                    self.phase = NefPhase::Air;
                } else if self.phase == NefPhase::Air && boss_hp < 0.20 {
                    self.phase = NefPhase::FinalGround;
                }
            }
            _ => {}
        }
    }
}

pub struct BlackwingLairFsm {
    active_boss: ActiveBoss,
    nef:         NefFsm,
    simple:      SimpleFsm,
}

impl BlackwingLairFsm {
    pub fn new() -> Self {
        Self {
            active_boss: ActiveBoss::None,
            nef:         NefFsm::new(),
            simple:      SimpleFsm::new(0),
        }
    }

    pub fn set_active_boss_by_entry(&mut self, entry: u32) {
        self.active_boss = match entry {
            ENTRY_RAZORGORE   => ActiveBoss::Razorgore,
            ENTRY_VAELASTRASZ => ActiveBoss::Vaelastrasz,
            ENTRY_BROODLORD   => ActiveBoss::Broodlord,
            ENTRY_FIREMAW     => ActiveBoss::Firemaw,
            ENTRY_EBONROC     => ActiveBoss::Ebonroc,
            ENTRY_FLAMEGOR    => ActiveBoss::Flamegor,
            ENTRY_CHROMAGGUS  => ActiveBoss::Chromaggus,
            ENTRY_NEFARIAN    => ActiveBoss::Nefarian,
            _                 => return,
        };
    }
}

impl Default for BlackwingLairFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for BlackwingLairFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        match self.active_boss {
            ActiveBoss::Nefarian => self.nef.update_inner(event, boss_hp_pct, time_ms),
            _                    => self.simple.update(event, boss_hp_pct, time_ms),
        }
    }

    fn phase_id(&self) -> u32 {
        if self.active_boss == ActiveBoss::Nefarian {
            match self.nef.phase {
                NefPhase::Idle         => 0,
                NefPhase::Ground       => 1,
                NefPhase::Air          => 2,
                NefPhase::FinalGround  => 3,
            }
        } else {
            self.simple.phase_id()
        }
    }

    fn is_active(&self) -> bool { self.active_boss != ActiveBoss::None }
    fn is_done(&self)   -> bool { false }

    fn boss_entry(&self) -> u32 {
        match self.active_boss {
            ActiveBoss::Razorgore   => ENTRY_RAZORGORE,
            ActiveBoss::Vaelastrasz => ENTRY_VAELASTRASZ,
            ActiveBoss::Broodlord   => ENTRY_BROODLORD,
            ActiveBoss::Firemaw     => ENTRY_FIREMAW,
            ActiveBoss::Ebonroc     => ENTRY_EBONROC,
            ActiveBoss::Flamegor    => ENTRY_FLAMEGOR,
            ActiveBoss::Chromaggus  => ENTRY_CHROMAGGUS,
            ActiveBoss::Nefarian    => ENTRY_NEFARIAN,
            ActiveBoss::None        => 0,
        }
    }
}
