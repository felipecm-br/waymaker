//! Mermaid diagrams from Markdown previews, as Unicode or ASCII text: rendering never panics.
#![no_main]
use libfuzzer_sys::fuzz_target;
use waymaker::utils::mermaid::{MermaidOptions, render_mermaid_ansi};

fuzz_target!(|input: (u8, bool, bool, &str)| {
    let (width, ascii, show_box, src) = input;
    let opts = MermaidOptions {
        max_width: (width > 0).then_some(width as usize),
        ascii,
        show_box,
        ..Default::default()
    };
    let _ = render_mermaid_ansi(src, &opts);
});
