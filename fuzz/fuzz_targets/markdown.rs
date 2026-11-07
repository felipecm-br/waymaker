//! The Markdown preview renders whatever file is under the cursor: it never panics, at any
//! width. Inline images are off so that links never read files.
#![no_main]
use libfuzzer_sys::fuzz_target;
use waymaker::utils::markdown::{MarkdownOptions, render_markdown_ansi};

fuzz_target!(|input: (u8, bool, &str)| {
    let (width, line_numbers, src) = input;
    let opts = MarkdownOptions {
        max_width: (width > 0).then_some(width as usize),
        show_line_numbers: line_numbers,
        inline_images: false,
        ..Default::default()
    };
    let _ = render_markdown_ansi(src, &opts);
});
