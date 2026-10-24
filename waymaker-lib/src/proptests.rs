//! Property tests for the hand-written parsers that read user input: bind
//! triggers, the picker query, status templates and escape substitution.

use std::{str::FromStr, sync::Arc};

use proptest::prelude::*;

use crate::{
    binds::Trigger, nucleo::query::PickerQuery, ui::results::status::StatusUI,
    utils::string::substitute_escaped,
};

/// Strings shaped like bind triggers, so the parser accepts a useful share of
/// them: an optional `mode^^` prefix, modifiers, and a key, mouse or semantic name.
fn trigger_like() -> impl Strategy<Value = String> {
    let modifier = prop_oneof!["ctrl", "alt", "shift", "super"];
    let sep = prop_oneof![Just("-"), Just("+")];
    let key = prop_oneof![
        "[a-z0-9]",
        "f[1-9]",
        "enter|esc|tab|space|backspace|delete|home|end|left|right|up|down",
        "left|middle|right|scrollup|scrolldown|scrollleft|scrollright",
    ];
    let keyed = (prop::collection::vec((modifier, sep), 0..3), key).prop_map(|(mods, key)| {
        let mut s: String = mods.into_iter().map(|(m, s)| m + s).collect();
        s.push_str(&key);
        s
    });
    let semantic = "@[a-z0-9 ._:/+$@-]{1,8}";
    let kind = prop_oneof![keyed, semantic];
    (prop::option::of("[a-z0-9,]{1,6}"), kind).prop_map(|(mode, kind)| match mode {
        Some(mode) => format!("{mode}^^{kind}"),
        None => kind,
    })
}

/// Arbitrary text weighted toward the characters the parsers treat specially.
fn special_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            4 => Just('\\'),
            4 => Just('%'),
            4 => Just(' '),
            3 => Just('{'),
            3 => Just('}'),
            2 => Just(':'),
            2 => Just(','),
            8 => any::<char>(),
        ],
        0..40,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    #[test]
    fn trigger_parse_never_panics(s in any::<String>()) {
        let _ = Trigger::from_str(&s);
    }

    /// Triggers are serialized with `to_string`, so a parsed trigger must parse
    /// back to itself or saved configs would not load.
    #[test]
    fn trigger_display_round_trips(s in trigger_like()) {
        if let Ok(trigger) = Trigger::from_str(&s) {
            let shown = trigger.to_string();
            prop_assert_eq!(Trigger::from_str(&shown), Ok(trigger), "{:?} displayed as {:?}", s, shown);
        }
    }

    #[test]
    fn query_parse_never_panics(
        input in special_text(),
        columns in prop::collection::vec("[a-z]{0,4}", 1..4),
        primary in 0usize..4,
    ) {
        let primary = primary % columns.len();
        let mut query = PickerQuery::new(columns.into_iter().map(Arc::from), primary);
        query.parse(&input);
    }

    #[test]
    fn status_template_never_panics(s in special_text()) {
        let _ = StatusUI::parse_template_to_status_line(&s);
    }

    /// With nothing to substitute, escapes are left exactly as written.
    #[test]
    fn substitute_escaped_without_keys_is_identity(s in special_text()) {
        prop_assert_eq!(substitute_escaped::<&str>(&s, &[]), s);
    }

    #[test]
    fn substitute_escaped_only_touches_escaped_keys(s in "[^\\\\]*") {
        prop_assert_eq!(substitute_escaped(&s, &[('n', "\n")]), s);
    }
}
