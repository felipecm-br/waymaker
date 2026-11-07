//! Navigation-mode binds parsed into key combinations (unified binds, step 2b).
//!
//! `[nav.binds]` keys are strings today, matched against the event loop's key names with
//! special cases (case-insensitive names, `space` vs `" "`, `?` vs `shift-?`). Parsing them into
//! normalized `KeyCombination`s makes equal keys equal, and strings such as `gg` that are not a
//! key name become key sequences. Not wired into the render path yet: the differential test
//! below checks it agrees with the string matching in `keys::get_nav_bind` first.

use std::collections::HashMap;
use std::str::FromStr;

use crokey::KeyCombination;

use crate::action::{Actions, NullActionExt};

/// A nav-bind key: one key, or a sequence such as `gg`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum NavKey {
    Single(KeyCombination),
    Sequence(Vec<KeyCombination>),
}

/// Parses a single key as written in config or produced by the event loop.
fn parse_one(s: &str) -> Option<KeyCombination> {
    use crossterm::event::{KeyCode, KeyModifiers};
    let s = if s == " " { "space" } else { s };
    // crokey reads "J" as plain `j`; a lone capital letter means shift + that letter.
    let mut key = match s.chars().collect::<Vec<_>>().as_slice() {
        [c] if c.is_uppercase() => KeyCombination::new(
            KeyCode::Char(c.to_lowercase().next().unwrap_or(*c)),
            KeyModifiers::SHIFT,
        ),
        _ => KeyCombination::from_str(s).ok()?,
    }
    .normalized();
    // A symbol already carries its shift (`?` is shift-/ on most layouts), so `shift-?` and
    // `?` are one key. Letters keep it: crokey normalizes `shift-j` to `J`.
    if key.modifiers == KeyModifiers::SHIFT
        && key.codes.len() == 1
        && matches!(key.codes.first(), KeyCode::Char(c) if !c.is_alphabetic())
    {
        key.modifiers = KeyModifiers::NONE;
    }
    Some(key)
}

/// Parses a `[nav.binds]` key: a key name or combination, else a sequence of single characters.
pub(super) fn parse_nav_key(s: &str) -> Option<NavKey> {
    if let Some(k) = parse_one(s) {
        return Some(NavKey::Single(k));
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    chars
        .iter()
        .map(|c| parse_one(&c.to_string()))
        .collect::<Option<Vec<_>>>()
        .map(NavKey::Sequence)
}

/// `[nav.binds]` with parsed keys.
#[derive(Debug, Default)]
pub(super) struct NavKeymap {
    binds: HashMap<NavKey, Actions<NullActionExt>>,
    /// The keys exactly as written, for checks that compare spellings.
    spellings: std::collections::HashSet<String>,
}

impl NavKeymap {
    pub(super) fn from_strings(map: &HashMap<String, Actions<NullActionExt>>) -> Self {
        let binds = map
            .iter()
            .filter_map(|(k, v)| parse_nav_key(k).map(|key| (key, v.clone())))
            .collect();
        Self {
            binds,
            spellings: map.keys().cloned().collect(),
        }
    }

    /// Whether `key` is bound with exactly this spelling.
    pub(super) fn contains_spelling(&self, key: &str) -> bool {
        self.spellings.contains(key)
    }

    /// The actions bound to `key`, a key name as the event loop reports it.
    pub(super) fn get(&self, key: &str) -> Option<&Actions<NullActionExt>> {
        self.binds.get(&parse_nav_key(key)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use proptest::prelude::*;

    /// Key names in the spellings that appear in configs and in the event loop's reports.
    const KEYS: &[&str] = &[
        "j",
        "k",
        "g",
        "G",
        "J",
        "K",
        "t",
        "b",
        "q",
        "?",
        "shift-?",
        " ",
        "space",
        "Space",
        "esc",
        "Esc",
        "enter",
        "Enter",
        "tab",
        "backspace",
        "Backspace",
        "left",
        "Left",
        "ctrl-r",
        "Ctrl-r",
        "ctrl-u",
        "shift-j",
        "gg",
        "gt",
        "gb",
        "/",
        ",",
        ".",
        "\\",
        "|",
    ];

    /// The spellings `get_nav_bind` treats as one key: names of several characters
    /// case-insensitively, `space`/`" "`, `?`/`shift-?`; plus `shift-x`/`X`, which the keymap
    /// makes one key on purpose. Two of these bound at once may resolve either way.
    fn old_spelling(s: &str) -> String {
        match s {
            " " => "space".into(),
            "shift-?" => "?".into(),
            _ if s.starts_with("shift-") && s.chars().count() == 7 => s[6..].to_uppercase(),
            _ if s.chars().count() > 1 => s.to_ascii_lowercase(),
            _ => s.into(),
        }
    }

    fn binds() -> impl Strategy<Value = HashMap<String, Actions<NullActionExt>>> {
        prop::collection::hash_map(
            prop::sample::select(KEYS).prop_map(str::to_string),
            (0i32..100).prop_map(|n| Actions::from([Action::Pos(n)])),
            0..12,
        )
    }

    #[test]
    fn sequences_and_names_parse() {
        let g = parse_one("g").unwrap();
        assert_eq!(parse_nav_key("gg"), Some(NavKey::Sequence(vec![g, g])));
        assert_eq!(parse_nav_key("esc"), parse_nav_key("Esc"));
        assert_eq!(parse_nav_key(" "), parse_nav_key("space"));
        assert_eq!(parse_nav_key("shift-?"), parse_nav_key("?"));
        assert_eq!(parse_nav_key("shift-j"), parse_nav_key("J"));
        assert!(matches!(parse_nav_key("ctrl-r"), Some(NavKey::Single(_))));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// Differential: for every key the old string matching finds a bind for, the parsed
        /// keymap finds the same actions.
        #[test]
        fn keymap_finds_what_string_matching_finds(
            map in binds(),
            key in prop::sample::select(KEYS),
        ) {
            let keymap = NavKeymap::from_strings(&map);
            if let Some(old) = super::super::keys::get_nav_bind(&map, key) {
                let new = keymap.get(key);
                prop_assert!(
                    new.is_some(),
                    "string matching binds {key:?} but the keymap does not; map keys: {:?}",
                    map.keys().collect::<Vec<_>>()
                );
                // Two spellings of one key can both be bound; then either binding is a valid
                // answer, so only require that the keymap's answer is one of them.
                let new = new.unwrap();
                let spelled_alike: Vec<_> = map
                    .iter()
                    .filter(|(k, _)| old_spelling(k) == old_spelling(key))
                    .map(|(_, v)| v)
                    .collect();
                prop_assert!(
                    spelled_alike.contains(&new) || new == old,
                    "{key:?}: old {old:?}, new {new:?}"
                );
            }
        }
    }
}
