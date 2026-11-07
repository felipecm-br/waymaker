//! Bind triggers (`[binds]` keys, `-o b.<trigger>=...`): parsing never panics, and a parsed
//! trigger displays as text that parses back to itself, since configs are saved that way.
//!
//! Chords of several keys (`a-b`, kitty keyboard protocol only) are only checked for panics:
//! crokey drops and reorders keys in chords and applies Shift to them unevenly, so their text
//! form is not stable.
#![no_main]
use std::str::FromStr;

use libfuzzer_sys::fuzz_target;
use waymaker::binds::{Trigger, TriggerKind};

fuzz_target!(|s: &str| {
    if let Ok(trigger) = Trigger::from_str(s) {
        if matches!(&trigger.kind, TriggerKind::Key(k) if k.codes.len() > 1) {
            return;
        }
        let shown = trigger.to_string();
        assert_eq!(
            Trigger::from_str(&shown).as_ref(),
            Ok(&trigger),
            "{s:?} displayed as {shown:?}"
        );
    }
});
