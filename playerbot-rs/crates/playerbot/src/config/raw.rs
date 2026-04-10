//! Low-level parser for CMaNGOS-style `.conf` files.
//!
//! Ports the subset of `Config/Config.{h,cpp}` behaviour that
//! `PlayerbotAIConfig::Initialize()` relies on:
//!
//! * Key/value pairs with `#` line comments and optional `[Section]`
//!   headers (headers are decorative — `CMaNGOS` `Config` treats them as
//!   such, and so do we).
//! * Quoted and unquoted string values.
//! * Case-insensitive key lookup (keys are stored lowercase).
//! * Environment-variable overrides (`SetSource(path, "PlayerBots_")` in
//!   the C++ version).
//! * Typed getters with defaults (`get_bool`, `get_u32`, `get_i32`,
//!   `get_f32`, `get_string`).
//! * Comma-separated list getters (`get_u32_list`, `get_string_list`)
//!   matching the `LoadList` / `LoadListString` helpers in the original.
//! * Prefix iteration (`keys_with_prefix`) matching `ConfigAccess::GetValues`,
//!   used to enumerate `AiPlayerbot.WorldBuff*` and
//!   `AiPlayerbot.LoginCriteria*` dynamic keys.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::log_warn;

/// Parsed key/value store. Keys are stored lowercase; values retain
/// original casing.
#[derive(Debug, Default, Clone)]
pub struct RawConfig {
    entries: HashMap<String, String>,
}

impl RawConfig {
    /// Construct from an iterator of `(key, value)` pairs. Keys are
    /// lowercased as they are inserted. Later entries overwrite earlier
    /// ones for the same key.
    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut entries = HashMap::new();
        for (k, v) in pairs {
            entries.insert(k.into().to_lowercase(), v.into());
        }
        Self { entries }
    }

    /// Parse a file from disk.
    pub fn parse_file(path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(Self::parse_str(&content))
    }

    /// Parse configuration from a string buffer.
    pub fn parse_str(content: &str) -> Self {
        let mut entries: HashMap<String, String> = HashMap::new();

        for raw_line in content.lines() {
            // Strip trailing `\r` for Windows line endings.
            let mut line = raw_line.trim_end_matches('\r').trim();
            if line.is_empty() {
                continue;
            }
            // Line comments — `#` at the start of the trimmed line.
            if line.starts_with('#') {
                continue;
            }
            // Section headers — `[Foo]`. CMaNGOS Config treats these as
            // decorative; we do too.
            if line.starts_with('[') && line.ends_with(']') {
                continue;
            }

            // Split on the first '=' sign.
            let Some(eq) = line.find('=') else {
                continue;
            };
            let key = line[..eq].trim();
            let mut value = line[eq + 1..].trim();

            // Strip inline comment: an unquoted '#' terminates the value.
            // This matches CMaNGOS Config's behaviour — the in-tree
            // `.conf` files rely on it for trailing notes like
            // `AiPlayerbot.Enabled = 1  # enable me`.
            //
            // Quoted strings (`"...#..."`) keep the hash character.
            if !(value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
                && let Some(hash) = value.find('#')
            {
                value = value[..hash].trim();
            }

            // Strip matching quotes if present.
            let stripped = if value.len() >= 2
                && value.starts_with('"')
                && value.ends_with('"')
            {
                &value[1..value.len() - 1]
            } else {
                value
            };

            entries.insert(key.to_lowercase(), stripped.to_string());

            // Reborrow to discard `line` mutability.
            let _ = &mut line;
        }

        Self { entries }
    }

    /// Apply environment-variable overrides. Mirrors `CMaNGOS`
    /// `SetSource(path, prefix)` behaviour: for each env var whose name
    /// begins with `prefix`, the suffix is lower-cased and installed as
    /// a key, overwriting any file-loaded value.
    ///
    /// Example: `prefix = "PlayerBots_"` turns
    /// `PlayerBots_AiPlayerbot.Enabled=1` into an override for
    /// `aiplayerbot.enabled`.
    pub fn apply_env_overrides(&mut self, prefix: &str) {
        for (name, value) in std::env::vars() {
            if let Some(suffix) = name.strip_prefix(prefix) {
                self.entries.insert(suffix.to_lowercase(), value);
            }
        }
    }

    /// Get a boolean value with a default. Accepts `1`/`0`,
    /// `true`/`false`, `yes`/`no`, `on`/`off` (case-insensitive).
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        let Some(v) = self.entries.get(&key.to_lowercase()) else {
            return default;
        };
        match v.trim().to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            other => {
                log_warn!("config: key '{key}' has non-bool value '{other}', using default");
                default
            }
        }
    }

    /// Get an unsigned 32-bit integer with a default. Negative numbers
    /// log a warning and return the default.
    pub fn get_u32(&self, key: &str, default: u32) -> u32 {
        let Some(v) = self.entries.get(&key.to_lowercase()) else {
            return default;
        };
        match v.trim().parse::<i64>() {
            Ok(n) if (0..=u32::MAX as i64).contains(&n) => n as u32,
            _ => {
                log_warn!("config: key '{key}' has non-u32 value '{v}', using default");
                default
            }
        }
    }

    /// Get a signed 32-bit integer with a default.
    pub fn get_i32(&self, key: &str, default: i32) -> i32 {
        let Some(v) = self.entries.get(&key.to_lowercase()) else {
            return default;
        };
        if let Ok(n) = v.trim().parse::<i32>() {
            n
        } else {
            log_warn!("config: key '{key}' has non-i32 value '{v}', using default");
            default
        }
    }

    /// Get a 32-bit float with a default.
    pub fn get_f32(&self, key: &str, default: f32) -> f32 {
        let Some(v) = self.entries.get(&key.to_lowercase()) else {
            return default;
        };
        // Strip trailing `f` that C++ float literals often use, so `1.5f`
        // parses the same as `1.5`.
        let trimmed = v.trim();
        let cleaned = trimmed.strip_suffix('f').unwrap_or(trimmed);
        if let Ok(n) = cleaned.parse::<f32>() {
            n
        } else {
            log_warn!("config: key '{key}' has non-f32 value '{v}', using default");
            default
        }
    }

    /// Get a string value with a default. The returned reference is
    /// borrowed from the config store; clone if you need ownership.
    pub fn get_string<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.entries
            .get(&key.to_lowercase())
            .map_or(default, String::as_str)
    }

    /// Parse a comma-separated list of `u32`. Empty entries are skipped,
    /// matching `LoadList<std::list<uint32>>` in the C++ source.
    /// Unparseable entries are logged and dropped.
    pub fn get_u32_list(&self, key: &str) -> Vec<u32> {
        self.get_u32_list_default(key, "")
    }

    /// Like `get_u32_list` but uses a default CSV string when the key is
    /// missing. Matches `LoadList(config.GetStringDefault(key, default),
    /// out)` from the C++ source.
    pub fn get_u32_list_default(&self, key: &str, default_csv: &str) -> Vec<u32> {
        let raw = self
            .entries
            .get(&key.to_lowercase())
            .map_or(default_csv, String::as_str);
        parse_u32_csv(key, raw)
    }

    /// Parse a comma-separated list of strings. Empty entries are skipped.
    /// Matches `LoadListString<std::list<std::string>>` from the C++ source.
    pub fn get_string_list(&self, key: &str) -> Vec<String> {
        self.get_string_list_default(key, "")
    }

    /// Like `get_string_list` but uses a default CSV when the key is missing.
    pub fn get_string_list_default(&self, key: &str, default_csv: &str) -> Vec<String> {
        let raw = self
            .entries
            .get(&key.to_lowercase())
            .map_or(default_csv, String::as_str);
        parse_string_csv(raw)
    }

    /// Return all keys that contain `needle` as a substring (after
    /// lowercasing). Matches `ConfigAccess::GetValues(name)` from the
    /// original, which used `find` for substring match rather than a
    /// prefix. Results are sorted deterministically so callers like
    /// `PlayerbotAIConfig::Initialize()` that iterated in sorted order
    /// preserve their behaviour.
    pub fn keys_containing(&self, needle: &str) -> Vec<String> {
        let needle_lc = needle.to_lowercase();
        let mut out: Vec<String> = self
            .entries
            .keys()
            .filter(|k| k.contains(&needle_lc))
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// Expose the number of stored entries (diagnostics / tests).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no keys are stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn parse_u32_csv(key: &str, raw: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for item in raw.split(',') {
        let t = item.trim();
        if t.is_empty() {
            continue;
        }
        match t.parse::<u32>() {
            Ok(n) => out.push(n),
            Err(_) => {
                log_warn!("config: key '{key}' has non-u32 list item '{t}', skipping");
            }
        }
    }
    out
}

fn parse_string_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_file() {
        let c = RawConfig::parse_str("");
        assert!(c.is_empty());
        assert_eq!(c.get_bool("missing", true), true);
        assert_eq!(c.get_u32("missing", 42), 42);
        assert_eq!(c.get_string("missing", "dflt"), "dflt");
    }

    #[test]
    fn parses_comment_lines() {
        let c = RawConfig::parse_str(
            "# comment line\n\
             ## double\n\
             Key = value  # trailing comment\n",
        );
        assert_eq!(c.len(), 1);
        assert_eq!(c.get_string("Key", ""), "value");
    }

    #[test]
    fn parses_section_header() {
        let c = RawConfig::parse_str("[AiPlayerbotConf]\nKey = 1\n");
        assert_eq!(c.len(), 1);
        assert_eq!(c.get_u32("Key", 0), 1);
    }

    #[test]
    fn parses_whitespace_around_equals() {
        let c = RawConfig::parse_str(
            "K1=a\n\
             K2 =b\n\
             K3= c\n\
             K4 = d\n",
        );
        assert_eq!(c.get_string("K1", ""), "a");
        assert_eq!(c.get_string("K2", ""), "b");
        assert_eq!(c.get_string("K3", ""), "c");
        assert_eq!(c.get_string("K4", ""), "d");
    }

    #[test]
    fn parses_quoted_string() {
        let c = RawConfig::parse_str(r#"Key = "hello world""#);
        assert_eq!(c.get_string("Key", ""), "hello world");
    }

    #[test]
    fn quoted_string_keeps_hash_character() {
        // A quoted value must not be truncated at `#` (it's the literal
        // character `#`, not a comment marker inside quotes).
        let c = RawConfig::parse_str(r#"Key = "gnomes#1""#);
        assert_eq!(c.get_string("Key", ""), "gnomes#1");
    }

    #[test]
    fn case_insensitive_keys() {
        let c = RawConfig::parse_str("AiPlayerbot.Enabled = 1");
        assert_eq!(c.get_bool("aiplayerbot.enabled", false), true);
        assert_eq!(c.get_bool("AIPLAYERBOT.ENABLED", false), true);
        assert_eq!(c.get_bool("AiPlayerbot.Enabled", false), true);
    }

    #[test]
    fn env_var_overrides() {
        // The env var name must match the file key after the prefix is
        // stripped and lowercased. Env var names in POSIX shells can't
        // easily contain dots, so this test deliberately uses an
        // underscore-only key (the apply_env_overrides semantics are
        // "literal suffix match, lowercased", not "dot-to-underscore
        // translation").
        let env_name = "PLAYERBOTS_test_envprefix_key_rb4a";
        // SAFETY: set_var/remove_var are unsafe on Rust 2024; we guard
        // the usage with a single-thread test and clean up afterwards.
        unsafe {
            std::env::set_var(env_name, "42");
        }
        let mut c = RawConfig::parse_str("test_envprefix_key_rb4a = 10");
        c.apply_env_overrides("PLAYERBOTS_");
        assert_eq!(c.get_u32("test_envprefix_key_rb4a", 0), 42);
        unsafe {
            std::env::remove_var(env_name);
        }
    }

    #[test]
    fn typed_list_parsing() {
        let c = RawConfig::parse_str("Key = 1,2,3");
        assert_eq!(c.get_u32_list("Key"), vec![1, 2, 3]);
        let empty = RawConfig::parse_str("Empty = ");
        assert_eq!(empty.get_u32_list("Empty"), Vec::<u32>::new());
    }

    #[test]
    fn typed_list_with_spaces() {
        let c = RawConfig::parse_str("Key = 1, 2, 3");
        assert_eq!(c.get_u32_list("Key"), vec![1, 2, 3]);
    }

    #[test]
    fn string_list_parsing() {
        let c = RawConfig::parse_str("Names = alpha,beta, gamma");
        assert_eq!(
            c.get_string_list("Names"),
            vec!["alpha".to_string(), "beta".into(), "gamma".into()]
        );
    }

    #[test]
    fn unknown_key_returns_default() {
        let c = RawConfig::parse_str("");
        assert_eq!(c.get_u32("missing", 42), 42);
        assert_eq!(c.get_i32("missing", -7), -7);
        assert_eq!(c.get_f32("missing", 1.5), 1.5);
        assert_eq!(c.get_bool("missing", true), true);
        assert_eq!(c.get_string("missing", "x"), "x");
    }

    #[test]
    fn malformed_value_returns_default() {
        let c = RawConfig::parse_str("Key = not_a_number");
        assert_eq!(c.get_u32("Key", 99), 99);
        assert_eq!(c.get_i32("Key", -1), -1);
        assert_eq!(c.get_f32("Key", 1.5), 1.5);
    }

    #[test]
    fn bool_accepts_multiple_forms() {
        let c = RawConfig::parse_str(
            "A = 1\nB = true\nC = yes\nD = on\n\
             E = 0\nF = false\nG = no\nH = off\n",
        );
        for k in ["A", "B", "C", "D"] {
            assert_eq!(c.get_bool(k, false), true, "{k} should be true");
        }
        for k in ["E", "F", "G", "H"] {
            assert_eq!(c.get_bool(k, true), false, "{k} should be false");
        }
    }

    #[test]
    fn float_with_trailing_f_suffix() {
        let c = RawConfig::parse_str("Pi = 3.14f\nNo = 2.718");
        assert!((c.get_f32("Pi", 0.0) - 3.14).abs() < 0.001);
        assert!((c.get_f32("No", 0.0) - 2.718).abs() < 0.001);
    }

    #[test]
    fn keys_containing_returns_sorted() {
        let c = RawConfig::parse_str(
            "AiPlayerbot.WorldBuff.0.0 = 1\n\
             AiPlayerbot.WorldBuff.1.0.0 = 2\n\
             AiPlayerbot.Something.Else = 3\n",
        );
        let hits = c.keys_containing("AiPlayerbot.WorldBuff");
        assert_eq!(hits.len(), 2);
        // Both hits start with the lowercased prefix.
        assert!(hits[0].starts_with("aiplayerbot.worldbuff"));
        assert!(hits[1].starts_with("aiplayerbot.worldbuff"));
        // Sorted deterministically.
        assert!(hits[0] < hits[1]);
    }

    #[test]
    fn from_pairs_builder() {
        let c = RawConfig::from_pairs([("One", "1"), ("TWO", "two")]);
        assert_eq!(c.get_u32("one", 0), 1);
        assert_eq!(c.get_string("two", ""), "two");
    }

    #[test]
    fn later_key_overwrites_earlier() {
        let c = RawConfig::parse_str("K = 1\nK = 2\n");
        assert_eq!(c.get_u32("K", 0), 2);
    }

    #[test]
    fn crlf_line_endings_work() {
        let c = RawConfig::parse_str("K = 1\r\nL = 2\r\n");
        assert_eq!(c.get_u32("K", 0), 1);
        assert_eq!(c.get_u32("L", 0), 2);
    }
}
