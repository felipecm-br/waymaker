//! Status-line templates (`{...}` groups, `%` escapes): parsing never panics.
#![no_main]
use libfuzzer_sys::fuzz_target;
use waymaker::ui::results::status::StatusUI;

fuzz_target!(|s: &str| {
    let _ = StatusUI::parse_template_to_status_line(s);
});
