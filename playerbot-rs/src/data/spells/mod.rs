// Spell ID tables are always compiled — they are just numeric constants.
// Feature flags gate behavior modules (encounters, class builds), not spell IDs.
pub mod vanilla;
pub mod tbc;
pub mod wotlk;

use crate::ffi::SpellId;

/// Case-insensitive spell-name → id lookup for the `cast <name>` chat
/// command. Only covers spells users actually type in raid chat
/// (taunts, interrupts, panic buttons, utility) — not exhaustive.
///
/// Input is expected to already be lowercased and trimmed. Underscores
/// and spaces are both accepted (`cast Mortal Strike`, `cast mortal_strike`).
pub fn lookup_spell_by_name(name: &str) -> Option<SpellId> {
    // Normalize: lowercase, collapse separators to spaces.
    let normalized: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    let key = normalized.split_whitespace().collect::<Vec<_>>().join(" ");

    let id: u32 = match key.as_str() {
        // ── Tanking / threat ─────────────────────────────────────
        "taunt"                  => 355,     // warrior
        "mocking blow"           => 694,
        "growl"                  => 6795,    // druid
        "righteous defense"      => 31789,   // paladin
        "hand of reckoning"      => 62124,   // paladin (wotlk)
        "challenging shout"      => 1161,
        "challenging roar"       => 5209,    // druid
        "dark command"           => 56222,   // DK

        // ── Interrupts ───────────────────────────────────────────
        "pummel"                 => 6552,
        "shield bash"            => 72,
        "kick"                   => 1766,
        "counterspell"           => 2139,    // mage
        "earth shock"            => 8042,
        "spell lock"             => 19244,   // warlock felhunter
        "silence"                => 15487,   // priest
        "mind freeze"            => 47528,   // DK

        // ── CC / utility ─────────────────────────────────────────
        "polymorph"              => 118,
        "sap"                    => 6770,
        "seduction"              => 6358,    // warlock succubus
        "banish"                 => 710,
        "fear"                   => 5782,
        "hibernate"              => 2637,
        "scare beast"            => 1513,
        "freezing trap"          => 1499,
        "entangling roots"       => 339,
        "hammer of justice"      => 853,

        // ── Panic buttons ────────────────────────────────────────
        "shield wall"            => 871,
        "last stand"             => 12975,
        "divine shield"          => 642,
        "divine protection"      => 498,
        "lay on hands"           => 633,
        "bubble"                 => 642,     // alias
        "ice block"              => 45438,
        "vanish"                 => 1856,
        "evasion"                => 5277,
        "cloak of shadows"       => 31224,
        "feign death"            => 5384,
        "dispersion"             => 47585,

        // ── Combat basics (commonly requested) ───────────────────
        "mortal strike"          => 12294,
        "bloodthirst"            => 23881,
        "shield slam"            => 23922,
        "heroic strike"          => 78,
        "cleave"                 => 845,
        "whirlwind"              => 1680,

        // ── Dispels / utility healing ────────────────────────────
        "dispel magic"           => 527,
        "cleanse"                => 4987,
        "purify"                 => 1152,
        "purge"                  => 370,
        "remove curse"           => 475,
        "abolish disease"        => 552,
        "power word shield"      => 17,

        _ => return None,
    };
    Some(SpellId(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_common_names() {
        assert_eq!(lookup_spell_by_name("taunt"),        Some(SpellId(355)));
        assert_eq!(lookup_spell_by_name("Taunt"),        Some(SpellId(355)));
        assert_eq!(lookup_spell_by_name("MORTAL STRIKE"), Some(SpellId(12294)));
        assert_eq!(lookup_spell_by_name("mortal_strike"), Some(SpellId(12294)));
        assert_eq!(lookup_spell_by_name("bubble"),       Some(SpellId(642)));
        assert_eq!(lookup_spell_by_name("nonsense"),     None);
    }
}
