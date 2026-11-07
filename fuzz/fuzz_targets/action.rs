//! Actions, as written in binds and printed by `Transform` commands (whose output can be any
//! text): parsing never panics.
#![no_main]
use std::str::FromStr;

use libfuzzer_sys::fuzz_target;
use waymaker::action::{Action, NullActionExt};

fuzz_target!(|s: &str| {
    let _ = Action::<NullActionExt>::from_str(s);
});
