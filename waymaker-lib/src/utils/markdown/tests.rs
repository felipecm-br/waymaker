use super::*;

#[test]
fn test_render_markdown_basic() {
    let md = "# Title\n\nThis is **bold** and *italic* text.\n\n- Item 1\n- Item 2";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    let joined = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("Title"));
    assert!(joined.contains("bold"));
    assert!(joined.contains("italic"));
    assert!(joined.contains("Item 1"));
}

#[test]
fn test_render_markdown_embedded_mermaid() {
    let md =
        "# Architecture\n\n```mermaid\ngraph LR\n    A[Client] --> B[Server]\n```\n\nNext steps.";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    let joined = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("Architecture"));
    assert!(joined.contains("Client"));
    assert!(joined.contains("Server"));
    assert!(joined.contains("Next steps."));
}

#[test]
fn test_render_markdown_table() {
    let md = "| Name | Status |\n|:---|---:|\n| Alice | Active |\n| Bob | Offline |\n";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    let joined = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("Name"));
    assert!(joined.contains("Status"));
    assert!(joined.contains("Alice"));
    assert!(joined.contains("Active"));
    assert!(joined.contains("┌"));
    assert!(joined.contains("└"));
}

#[test]
fn test_render_markdown_task_list() {
    let md = "- [ ] Unfinished task\n- [x] Finished task\n";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    let joined = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("[ ]"));
    assert!(joined.contains("[✓]"));
}

#[test]
fn test_render_markdown_ansi() {
    let md = "## Heading\n\nInline `code` snippet";
    let opts = MarkdownOptions::default();
    let ansi = render_markdown_ansi(md, &opts);
    assert!(ansi.contains("Heading"));
    assert!(ansi.contains("code"));
}

#[test]
fn test_render_markdown_word_wrap() {
    let md = "This is a long paragraph of words that should definitely be wrapped onto multiple lines when max_width is constrained.";
    let opts = MarkdownOptions {
        max_width: Some(40),
        ..Default::default()
    };
    let text = render_markdown(md, &opts);
    assert!(
        text.lines.len() > 1,
        "Expected multiple lines due to wrapping"
    );
    for line in &text.lines {
        let w: usize = line
            .spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        assert!(w <= 40, "Line width {w} exceeded max_width 40");
    }
}

#[test]
fn test_render_markdown_task_list_no_bullet_duplication() {
    let md = "- [ ] Unfinished task\n- [x] Finished task\n";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    let joined = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !joined.contains("• [ ]"),
        "Task list should not contain both bullet and checkbox"
    );
    assert!(
        !joined.contains("• [✓]"),
        "Task list should not contain both bullet and checkbox"
    );
    assert!(joined.contains("[ ] Unfinished task"));
    assert!(joined.contains("[✓] Finished task"));
}

#[test]
fn test_render_markdown_image() {
    let md = "![Architecture Diagram](https://example.com/arch.png)";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    let joined = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("🖼  [Architecture Diagram](https://example.com/arch.png)"));
}

#[test]
fn test_render_markdown_blockquote_multiline() {
    let md = "> Quote paragraph 1\n>\n> Quote paragraph 2";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    for line in &text.lines {
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            joined.starts_with('▌'),
            "Every blockquote line must have the vertical bar: {joined}"
        );
    }
}

#[test]
fn test_render_markdown_no_duplicate_blank_lines() {
    let md = "- Item 1\n- Item 2\n\nParagraph after list";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    let mut consecutive_blank = 0;
    for line in &text.lines {
        if is_line_blank(line) {
            consecutive_blank += 1;
            assert!(
                consecutive_blank <= 1,
                "Encountered more than one consecutive blank line"
            );
        } else {
            consecutive_blank = 0;
        }
    }
}

#[test]
fn test_render_markdown_code_block_symmetric_borders() {
    let md = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    let code_lines: Vec<&Line<'_>> = text.lines.iter().filter(|l| !is_line_blank(l)).collect();
    assert!(code_lines.len() >= 3);
    let widths: Vec<usize> = code_lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum()
        })
        .collect();
    let expected_w = widths[0];
    for (idx, w) in widths.iter().enumerate() {
        assert_eq!(
            *w, expected_w,
            "Line {idx} width ({w}) does not match top border width ({expected_w})"
        );
    }
}

#[test]
fn test_extract_mermaid_blocks() {
    let md = r#"# Architecture

Here is the flow:

```mermaid
graph TD
A --> B
```

And another:

~~~mermaid
sequenceDiagram
Alice->>Bob: Hello
~~~
"#;
    let blocks = extract_mermaid_blocks(md);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].0, 4); // line index of opening fence
    assert!(blocks[0].1.contains("A --> B"));
    assert_eq!(blocks[1].0, 11);
    assert!(blocks[1].1.contains("Alice->>Bob"));
}

#[test]
fn test_render_markdown_with_diagram_offsets() {
    let md = r#"# Document

Intro paragraph.

```mermaid
graph TD
X --> Y
```

Outro paragraph.
"#;
    let opts = MarkdownOptions::default();
    let (text, offsets) = render_markdown_with_diagram_offsets(md, &opts);
    assert!(
        !offsets.is_empty(),
        "Offsets must detect the diagram box header"
    );
    assert!(
        offsets[0] < text.lines.len(),
        "Offset must point within rendered text"
    );
}

#[test]
fn test_render_markdown_without_diagrams() {
    let md = r#"# Document

```mermaid
graph TD
X --> Y
```
"#;
    let opts = MarkdownOptions {
        render_mermaid: false,
        ..Default::default()
    };
    let (text, offsets) = render_markdown_with_diagram_offsets(md, &opts);
    assert!(
        offsets.is_empty(),
        "Offsets must be empty when render_mermaid is false"
    );
    let rendered_str = format!("{text:?}");
    assert!(
        !rendered_str.contains("Mermaid Diagram"),
        "Diagram box title should not appear when render_mermaid is false"
    );
    assert!(
        rendered_str.contains("X --> Y"),
        "Original code block content should still be rendered"
    );
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_render_markdown_inline_diagrams() {
    let _env_lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_DIAGRAMS").ok();
    unsafe {
        std::env::set_var("MM_INLINE_DIAGRAMS", "1");
    }

    let md = r#"# Document

```mermaid
graph LR
A --> B
```
"#;
    let opts = MarkdownOptions {
        inline_diagrams: true,
        max_width: Some(80),
        ..Default::default()
    };
    let mut parser_opts = Options::empty();
    parser_opts.insert(Options::ENABLE_TABLES);
    parser_opts.insert(Options::ENABLE_TASKLISTS);
    parser_opts.insert(Options::ENABLE_STRIKETHROUGH);
    parser_opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    let parser = Parser::new_ext(md, parser_opts);

    let mut renderer = MarkdownRenderer::new(&opts);
    renderer.render(parser);
    let (text, offsets, transmissions) = renderer.finish_with_transmissions();

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_DIAGRAMS", v),
            None => std::env::remove_var("MM_INLINE_DIAGRAMS"),
        }
    }

    assert!(
        !offsets.is_empty(),
        "Offsets must record the diagram position"
    );
    assert!(
        !transmissions.is_empty(),
        "Transmissions must be emitted for inline diagram"
    );
    assert!(
        transmissions[0].1.contains("U=1"),
        "Transmission must be Kitty Unicode placeholder format"
    );

    let rendered_str = format!("{text:?}");
    println!("rendered_str = {rendered_str}");
    let found_in_spans = text.lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains(super::super::mermaid::PLACEHOLDER))
    });
    assert!(
        !rendered_str.contains("inline diagram"),
        "Rendered text must NOT contain artificial inline diagram banner"
    );
    assert!(
        found_in_spans,
        "Rendered text spans must contain Kitty placeholder character"
    );
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_render_markdown_ansi_inline_diagrams() {
    let _env_lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_DIAGRAMS").ok();
    unsafe {
        std::env::set_var("MM_INLINE_DIAGRAMS", "1");
    }

    let md = r#"```mermaid
graph TD
X --> Y
```"#;
    let opts = MarkdownOptions {
        inline_diagrams: true,
        max_width: Some(70),
        ..Default::default()
    };
    let ansi = render_markdown_ansi(md, &opts);

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_DIAGRAMS", v),
            None => std::env::remove_var("MM_INLINE_DIAGRAMS"),
        }
    }

    assert!(
        ansi.contains("\u{10EEEE}"),
        "ANSI output must contain inline diagram placeholder"
    );
    assert!(
        !ansi.contains("inline diagram"),
        "ANSI output must NOT contain artificial inline diagram banner"
    );
    assert!(
        ansi.contains("_G"),
        "ANSI output must contain Kitty graphics escape sequence"
    );
}

#[test]
fn test_render_markdown_inline_local_image() {
    let _lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_IMAGES").ok();
    unsafe {
        std::env::set_var("MM_INLINE_IMAGES", "1");
    }

    // Create a temporary PNG image
    let temp_dir = std::env::temp_dir().join("mm_test_inline_img");
    let _ = std::fs::create_dir_all(&temp_dir);
    let img_path = temp_dir.join("test_img.png");

    let mut img_buf = image::RgbaImage::new(100, 100);
    for pixel in img_buf.pixels_mut() {
        *pixel = image::Rgba([255, 0, 0, 255]);
    }
    img_buf.save(&img_path).unwrap();

    let md = format!(
        "# Document\n\n![Test Image]({})\n\nAfter image",
        img_path.display()
    );
    let opts = MarkdownOptions {
        inline_images: true,
        max_width: Some(60),
        ..Default::default()
    };

    let text = render_markdown(&md, &opts);

    let found_placeholder = text.lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains(super::super::mermaid::PLACEHOLDER))
    });

    let ansi = render_markdown_ansi(&md, &opts);

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
    }
    let _ = std::fs::remove_dir_all(&temp_dir);

    assert!(
        found_placeholder,
        "Rendered markdown text must contain inline Kitty placeholder characters for local image"
    );
    assert!(
        ansi.contains("\u{10EEEE}"),
        "ANSI output must contain Unicode placeholder \\u{{10EEEE}}"
    );
    assert!(
        ansi.contains("_G"),
        "ANSI output must contain Kitty graphics escape sequence for transmitted image"
    );
    assert!(
        ansi.contains("U=1"),
        "Kitty graphics transmission must specify U=1 for Unicode placeholder mode"
    );
}

#[test]
fn test_render_markdown_image_relative_path_and_lru_cache() {
    let _lock = crate::utils::lock_test_env();
    clear_image_placeholder_cache();

    let prev = std::env::var("MM_INLINE_IMAGES").ok();
    unsafe {
        std::env::set_var("MM_INLINE_IMAGES", "1");
    }

    let temp_dir = std::env::temp_dir().join("mm_test_rel_img");
    let sub_dir = temp_dir.join("assets");
    let _ = std::fs::create_dir_all(&sub_dir);
    let img_path = sub_dir.join("pic.png");

    let mut img_buf = image::RgbaImage::new(80, 80);
    for pixel in img_buf.pixels_mut() {
        *pixel = image::Rgba([0, 255, 0, 255]);
    }
    img_buf.save(&img_path).unwrap();

    let md_file = temp_dir.join("readme.md");
    std::fs::write(&md_file, "![My Pic](./assets/pic.png)").unwrap();

    let opts = MarkdownOptions {
        inline_images: true,
        max_width: Some(50),
        ..Default::default()
    };

    // 1. First render: loads from disk, caches in LRU
    let (text1, _) = render_markdown_file_with_diagram_offsets(&md_file, &opts).unwrap();
    let placeholder1 = text1.lines.iter().find(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains(super::super::mermaid::PLACEHOLDER))
    });
    assert!(
        placeholder1.is_some(),
        "Relative image must be found and rendered inline"
    );

    // 2. Second render: must hit LRU cache with same image ID
    let (text2, _) = render_markdown_file_with_diagram_offsets(&md_file, &opts).unwrap();
    let placeholder2 = text2.lines.iter().find(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains(super::super::mermaid::PLACEHOLDER))
    });
    assert!(
        placeholder2.is_some(),
        "Second render must also succeed from LRU cache"
    );

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
    }
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_render_markdown_image_fallback_when_disabled_or_missing() {
    let _lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_IMAGES").ok();
    unsafe {
        std::env::set_var("MM_INLINE_IMAGES", "0");
    }

    let md = "![Test Pic](some_image.png)";
    let opts = MarkdownOptions::default();
    let text = render_markdown(md, &opts);
    let joined: String = text
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect();
    assert!(
        joined.contains("🖼  [Test Pic](some_image.png)"),
        "Must fall back to text representation when Kitty is disabled"
    );

    // When enabled but file is missing
    unsafe {
        std::env::set_var("MM_INLINE_IMAGES", "1");
    }
    let text_missing = render_markdown(md, &opts);
    let joined_missing: String = text_missing
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect();
    assert!(
        joined_missing.contains("🖼  [Test Pic](some_image.png)"),
        "Must cleanly fall back to text representation when image file does not exist"
    );

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
    }
}

#[test]
fn test_render_markdown_image_url_encoded_and_spaces() {
    let _lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_IMAGES").ok();
    unsafe {
        std::env::set_var("MM_INLINE_IMAGES", "1");
    }

    let temp_dir = std::env::temp_dir().join("mm_test_spaces_img");
    let _ = std::fs::create_dir_all(&temp_dir);
    let img_path = temp_dir.join("photo with spaces.png");

    let mut img_buf = image::RgbaImage::new(50, 50);
    for pixel in img_buf.pixels_mut() {
        *pixel = image::Rgba([0, 0, 255, 255]);
    }
    img_buf.save(&img_path).unwrap();

    // Reference using %20
    let md = "![Space Pic](photo%20with%20spaces.png)";
    let opts = MarkdownOptions {
        base_path: Some(temp_dir.join("doc.md")),
        inline_images: true,
        ..Default::default()
    };
    let text = render_markdown(md, &opts);
    let has_placeholder = text.lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains(super::super::mermaid::PLACEHOLDER))
    });

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
    }
    let _ = std::fs::remove_dir_all(&temp_dir);

    assert!(
        has_placeholder,
        "Percent-encoded image URLs must resolve on disk and render as inline Kitty placeholders"
    );
}

#[test]
fn test_render_markdown_image_data_uri() {
    let _lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_IMAGES").ok();
    unsafe {
        std::env::set_var("MM_INLINE_IMAGES", "1");
    }

    let mut img_buf = image::RgbaImage::new(10, 10);
    for pixel in img_buf.pixels_mut() {
        *pixel = image::Rgba([255, 255, 0, 255]);
    }
    let mut png_bytes = Vec::new();
    img_buf
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    let data_uri = format!("data:image/png;base64,{b64}");
    let md = format!("![Data URI]({data_uri})");

    let opts = MarkdownOptions {
        inline_images: true,
        ..Default::default()
    };
    let text = render_markdown(&md, &opts);
    let has_placeholder = text.lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains(super::super::mermaid::PLACEHOLDER))
    });

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
    }

    assert!(
        has_placeholder,
        "Data URIs must decode and render inline Kitty placeholders"
    );
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_render_markdown_image_disabled_flag_independence() {
    let _lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_IMAGES").ok();
    let prev_diag = std::env::var("MM_INLINE_DIAGRAMS").ok();
    unsafe {
        std::env::remove_var("MM_INLINE_IMAGES");
        std::env::set_var("MM_INLINE_DIAGRAMS", "1");
    }

    let temp_dir = std::env::temp_dir().join("mm_test_flag_indep");
    let _ = std::fs::create_dir_all(&temp_dir);
    let img_path = temp_dir.join("photo.png");

    let mut img_buf = image::RgbaImage::new(40, 40);
    for pixel in img_buf.pixels_mut() {
        *pixel = image::Rgba([100, 150, 200, 255]);
    }
    img_buf.save(&img_path).unwrap();

    let md = format!(
        "![Photo]({})\n\n```mermaid\ngraph TD;\nA-->B;\n```",
        img_path.display()
    );

    // inline_images = false, inline_diagrams = true
    let opts = MarkdownOptions {
        inline_images: false,
        inline_diagrams: true,
        ..Default::default()
    };

    let text = render_markdown(&md, &opts);
    let joined: String = text
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect();

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
        match prev_diag {
            Some(v) => std::env::set_var("MM_INLINE_DIAGRAMS", v),
            None => std::env::remove_var("MM_INLINE_DIAGRAMS"),
        }
    }
    let _ = std::fs::remove_dir_all(&temp_dir);

    // Image MUST be fallback text
    assert!(
        joined.contains("🖼  [Photo]"),
        "When inline_images=false, images must NOT be rendered inline even if inline_diagrams=true"
    );
    // Diagram MUST still have placeholder lines
    let has_diagram_placeholder = text.lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains(super::super::mermaid::PLACEHOLDER))
    });
    assert!(
        has_diagram_placeholder,
        "Diagram must still render inline when inline_diagrams=true"
    );
}

#[test]
fn test_is_inline_images_enabled_env_semantics() {
    let _lock = crate::utils::lock_test_env();
    let prev_wm = std::env::var("WM_INLINE_IMAGES").ok();
    let prev_mm = std::env::var("MM_INLINE_IMAGES").ok();
    let prev_diag = std::env::var("MM_INLINE_DIAGRAMS").ok();
    let prev_wm_diag = std::env::var("WM_INLINE_DIAGRAMS").ok();

    unsafe {
        std::env::remove_var("WM_INLINE_IMAGES");
        std::env::remove_var("MM_INLINE_IMAGES");
        std::env::remove_var("WM_INLINE_DIAGRAMS");
        std::env::remove_var("MM_INLINE_DIAGRAMS");
    }

    // 1. WM_INLINE_IMAGES=1 forces enabled regardless of opts
    unsafe {
        std::env::set_var("WM_INLINE_IMAGES", "1");
        std::env::set_var("WM_INLINE_DIAGRAMS", "0");
    }
    assert!(
        is_inline_images_enabled(false),
        "WM_INLINE_IMAGES=1 must force enable images even if opts.inline_images=false"
    );
    assert!(
        is_inline_images_enabled(true),
        "WM_INLINE_IMAGES=1 must force enable images even if WM_INLINE_DIAGRAMS=0"
    );

    // 2. WM_INLINE_IMAGES=0 forces disabled regardless of opts
    unsafe {
        std::env::set_var("WM_INLINE_IMAGES", "0");
        std::env::set_var("WM_INLINE_DIAGRAMS", "1");
    }
    assert!(
        !is_inline_images_enabled(true),
        "WM_INLINE_IMAGES=0 must force disable images even if opts.inline_images=true and diagrams=1"
    );

    // 3. Fallback to MM_INLINE_IMAGES=1
    unsafe {
        std::env::remove_var("WM_INLINE_IMAGES");
        std::env::set_var("MM_INLINE_IMAGES", "1");
        std::env::set_var("MM_INLINE_DIAGRAMS", "0");
    }
    assert!(
        is_inline_images_enabled(false),
        "MM_INLINE_IMAGES=1 fallback must force enable images"
    );

    // 4. WM_INLINE_DIAGRAMS=0 must NOT disable images if terminal supports it
    unsafe {
        std::env::remove_var("MM_INLINE_IMAGES");
        std::env::set_var("WM_INLINE_DIAGRAMS", "0");
    }
    if super::super::mermaid::is_kitty_terminal_supported() {
        assert!(
            is_inline_images_enabled(true),
            "WM_INLINE_DIAGRAMS=0 must not affect inline images when terminal is supported"
        );
    }

    unsafe {
        match prev_wm {
            Some(v) => std::env::set_var("WM_INLINE_IMAGES", v),
            None => std::env::remove_var("WM_INLINE_IMAGES"),
        }
        match prev_mm {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
        match prev_diag {
            Some(v) => std::env::set_var("MM_INLINE_DIAGRAMS", v),
            None => std::env::remove_var("MM_INLINE_DIAGRAMS"),
        }
        match prev_wm_diag {
            Some(v) => std::env::set_var("WM_INLINE_DIAGRAMS", v),
            None => std::env::remove_var("WM_INLINE_DIAGRAMS"),
        }
    }
}

#[test]
fn test_render_markdown_image_in_list_item_no_orphan_bullet() {
    let _lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_IMAGES").ok();
    unsafe {
        std::env::set_var("MM_INLINE_IMAGES", "1");
    }

    let temp_dir = std::env::temp_dir().join("mm_test_list_img");
    let _ = std::fs::create_dir_all(&temp_dir);
    let img_path = temp_dir.join("bullet_pic.png");

    let mut img_buf = image::RgbaImage::new(30, 30);
    for pixel in img_buf.pixels_mut() {
        *pixel = image::Rgba([12, 34, 56, 255]);
    }
    img_buf.save(&img_path).unwrap();

    let md = format!("- ![Bullet Pic]({})\n- Normal item", img_path.display());
    let opts = MarkdownOptions {
        inline_images: true,
        ..Default::default()
    };
    let text = render_markdown(&md, &opts);

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
    }
    let _ = std::fs::remove_dir_all(&temp_dir);

    // There should be NO line consisting solely of a bullet "• "
    for line in &text.lines {
        let line_str: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        let trimmed = line_str.trim();
        assert_ne!(
            trimmed, "•",
            "List item image must not produce an isolated orphan bullet line"
        );
    }
}

#[test]
fn test_render_markdown_image_file_scheme_and_relative_priority() {
    let _lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_IMAGES").ok();
    unsafe {
        std::env::set_var("MM_INLINE_IMAGES", "1");
    }

    let temp_dir = std::env::temp_dir().join("mm_test_file_scheme");
    let _ = std::fs::create_dir_all(&temp_dir);
    let img_path = temp_dir.join("photo.png");

    let mut img_buf = image::RgbaImage::new(20, 20);
    for pixel in img_buf.pixels_mut() {
        *pixel = image::Rgba([99, 88, 77, 255]);
    }
    img_buf.save(&img_path).unwrap();

    // 1. Test file://localhost/path
    let file_url = format!("file://localhost{}", img_path.display());
    let resolved = resolve_local_image_path(&file_url, None);
    assert_eq!(
        resolved,
        Some(std::fs::canonicalize(&img_path).unwrap()),
        "file://localhost/<abs_path> must resolve to absolute path with leading slash preserved"
    );

    // 2. Test relative priority over CWD
    let doc_dir = temp_dir.join("subdocs");
    let _ = std::fs::create_dir_all(&doc_dir);
    let doc_img = doc_dir.join("nested.png");
    let mut doc_img_buf = image::RgbaImage::new(20, 20);
    for pixel in doc_img_buf.pixels_mut() {
        *pixel = image::Rgba([1, 2, 3, 255]);
    }
    doc_img_buf.save(&doc_img).unwrap();

    let resolved_rel = resolve_local_image_path("nested.png", Some(&doc_dir.join("readme.md")));
    assert_eq!(
        resolved_rel,
        Some(std::fs::canonicalize(&doc_img).unwrap()),
        "Relative image path must resolve relative to document directory"
    );

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
    }
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_render_markdown_image_svg_data_uri() {
    let _lock = crate::utils::lock_test_env();
    let prev = std::env::var("MM_INLINE_IMAGES").ok();
    unsafe {
        std::env::set_var("MM_INLINE_IMAGES", "1");
    }

    let svg_data = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><rect width="40" height="40" fill="green"/></svg>"#;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg_data);
    let data_uri = format!("data:image/svg+xml;base64,{b64}");
    let md = format!("![SVG Data]({data_uri})");

    let opts = MarkdownOptions {
        inline_images: true,
        ..Default::default()
    };
    let text = render_markdown(&md, &opts);
    let has_placeholder = text.lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains(super::super::mermaid::PLACEHOLDER))
    });

    unsafe {
        match prev {
            Some(v) => std::env::set_var("MM_INLINE_IMAGES", v),
            None => std::env::remove_var("MM_INLINE_IMAGES"),
        }
    }

    assert!(
        has_placeholder,
        "SVG data URIs must decode and render inline Kitty placeholders"
    );
}
