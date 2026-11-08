use super::*;

#[test]
fn test_render_flowchart_basic() {
    let src = "graph TD\n    A[Start] --> B[End]";
    let opts = MermaidOptions::default();
    let text = render_mermaid(src, &opts);
    assert!(!text.lines.is_empty());
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
    assert!(joined.contains("Start"));
    assert!(joined.contains("End"));
}

#[test]
fn test_render_ascii_mode() {
    let src = "graph LR\n    A[Left] --> B[Right]";
    let mut opts = MermaidOptions::default();
    opts.ascii = true;
    let text = render_mermaid(src, &opts);
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
    assert!(joined.contains("Left"));
    assert!(joined.contains("Right"));
    assert!(joined.is_ascii());
}

#[test]
fn test_render_width_constrained() {
    let src = "graph LR\n    A[Start] --> B[Process] --> C[Done]";
    let mut opts = MermaidOptions::default();
    opts.max_width = Some(40);
    let text = render_mermaid(src, &opts);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_error_fallback() {
    let src = "this is not valid mermaid @#$*&";
    let opts = MermaidOptions::default();
    let text = render_mermaid(src, &opts);
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
    assert!(joined.contains("Mermaid Syntax Warning"));
    assert!(joined.contains("this is not valid mermaid"));
}

#[test]
fn test_render_empty() {
    let src = "   \n\t  ";
    let opts = MermaidOptions::default();
    let text = render_mermaid(src, &opts);
    assert!(text.lines.is_empty());
}

#[test]
fn class_diagram_with_non_ascii_names_does_not_panic() {
    // Found by the `mermaid` fuzz target (mermaid-text 0.57 sliced inside a multi-byte
    // character); with `panic = "abort"`, previewing such a file killed wm.
    for src in [
        "classDiagram\n    Usuário\n",
        "classDiagram\n    class Ação\n",
        "classDiagram\n ₔ│\n",
    ] {
        let _ = render_mermaid_ansi(src, &MermaidOptions::default());
    }
}

#[test]
fn test_render_ansi() {
    let src = "graph TD\n    A[NodeA] --> B[NodeB]";
    let opts = MermaidOptions::default();
    let ansi = render_mermaid_ansi(src, &opts);
    assert!(ansi.contains("NodeA"));
    assert!(ansi.contains("NodeB"));
}

#[test]
fn test_render_ascii_with_box_is_pure_ascii() {
    let src = "graph LR\n    A[Step 1] --> B[Step 2]";
    let opts = MermaidOptions {
        ascii: true,
        show_box: true,
        title: Some("ASCII Box Test".to_string()),
        ..Default::default()
    };
    let text = render_mermaid(src, &opts);
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
        joined.is_ascii(),
        "Rendered ASCII diagram with box must be pure ASCII"
    );
    assert!(joined.contains("+-[ ASCII Box Test ]"));
    assert!(joined.contains("Step 1"));
    assert!(joined.contains("Step 2"));
}

#[test]
fn test_render_box_symmetric_width() {
    let src = "graph LR\n    Alpha[Start Here] --> Beta[A Somewhat Longer Step] --> Gamma[Done]";
    let opts = MermaidOptions {
        show_box: true,
        title: Some("Symmetry Test".to_string()),
        ..Default::default()
    };
    let text = render_mermaid(src, &opts);
    let widths: Vec<usize> = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum()
        })
        .collect();
    assert!(!widths.is_empty());
    let expected_w = widths[0];
    for (idx, w) in widths.iter().enumerate() {
        assert_eq!(
            *w, expected_w,
            "Line {idx} width ({w}) does not match header width ({expected_w})"
        );
    }
}

#[test]
fn test_render_class_diagram() {
    let src = "classDiagram\n    Animal <|-- Duck";
    let opts = MermaidOptions::default();
    let text = render_mermaid(src, &opts);
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
    assert!(joined.contains("Animal"));
    assert!(joined.contains("Duck"));
}

#[test]
fn test_render_sequence_diagram() {
    let src = "sequenceDiagram\n    Alice->>Bob: Ping\n    Bob-->>Alice: Pong";
    let opts = MermaidOptions::default();
    let text = render_mermaid(src, &opts);
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
    assert!(joined.contains("Alice"));
    assert!(joined.contains("Bob"));
    assert!(joined.contains("Ping"));
    assert!(joined.contains("Pong"));
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_render_mermaid_to_image() {
    let src =
        "flowchart TD\n    A[Start] --> B{Choice}\n    B -->|Yes| C[OK]\n    B -->|No| D[Stop]";
    let img = render_mermaid_to_image(src, 1.0);
    assert!(
        img.is_some(),
        "Should successfully render flowchart to image"
    );
    let img = img.unwrap();
    assert!(img.width() > 0, "Image width must be positive");
    assert!(img.height() > 0, "Image height must be positive");

    // Test zoom / scaling
    let img_scaled = render_mermaid_to_image(src, 2.0).unwrap();
    assert!(
        img_scaled.width() > img.width(),
        "Scaled image width should be larger: {} vs {}",
        img_scaled.width(),
        img.width()
    );

    // Test LRU cache hit (should return identical dimensions instantly)
    let cached = render_mermaid_to_image(src, 1.0).unwrap();
    assert_eq!(cached.width(), img.width());
    assert_eq!(cached.height(), img.height());
}

#[test]
fn test_render_mermaid_to_image_empty() {
    assert!(render_mermaid_to_image("", 1.0).is_none());
    assert!(render_mermaid_to_image("   \n  ", 1.0).is_none());
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_git_worktree_diagram() {
    let src = r#"graph TD
BARE[".dotfiles/.bare (Git Bare Repository)"] --> MAIN[".dotfiles/main (Production Worktree / Stowed)"]
BARE --> WT1[".dotfiles/feat-zsh-perf (AI Sandbox 1)"]
BARE --> WT2[".dotfiles/feat-nvim-ui (AI Sandbox 2)"]

MAIN ===|./stow.sh| HOME["$HOME (~/.config, ~/.zsh, ~/.local/bin, etc.)"]
WT1 -.->|ISOLATED / NOT STOWED| HOME
WT2 -.->|ISOLATED / NOT STOWED| HOME"#;
    let img = render_mermaid_to_image(src, 1.0);
    println!(
        "Render result: {:?}",
        img.as_ref().map(|i| (i.width(), i.height()))
    );
    assert!(img.is_some(), "Worktree diagram should render to image");
}

#[test]
fn test_diacritics_table_coverage() {
    assert!(
        DIACRITICS.len() >= 256,
        "DIACRITICS table must have at least 256 entries"
    );
    for i in 0..DIACRITICS.len() {
        assert!(
            get_diacritic(i).is_some(),
            "Index {i} must yield a valid diacritic char"
        );
    }
}

#[test]
fn test_encode_kitty_unicode_transmission_format() {
    let fake_png = b"FAKE_PNG_BINARY_DATA_FOR_TESTING";
    let seq = encode_kitty_unicode_transmission(fake_png, 42, 60, 20);
    assert!(seq.contains("a=T"), "Must specify action transmit");
    assert!(seq.contains("f=100"), "Must specify format 100 (PNG)");
    assert!(seq.contains("i=42"), "Must include image ID");
    assert!(seq.contains("c=60"), "Must include columns");
    assert!(seq.contains("r=20"), "Must include rows");
    assert!(seq.contains("U=1"), "Must enable Unicode placeholder mode");
    assert!(seq.contains("q=2"), "Must set quiet mode");
}

#[test]
fn test_encode_kitty_delete_format() {
    let del = encode_kitty_delete(42);
    assert!(del.contains("a=d"), "Must specify action delete");
    assert!(del.contains("d=i"), "Must specify delete by image ID");
    assert!(del.contains("i=42"), "Must include image ID 42");
}

#[test]
fn test_encode_kitty_delete_all_format() {
    let del = encode_kitty_delete_all();
    assert!(del.contains("a=d"), "Must specify action delete");
    assert!(del.contains("d=a"), "Must specify delete all images");
}

#[test]
fn test_create_unicode_placeholder_lines() {
    let lines = create_unicode_placeholder_lines(12345, 10, 5, 2);
    assert_eq!(lines.len(), 5, "Must create exactly 5 lines for rows=5");
    for line in lines {
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(joined.starts_with("  "), "Must have 2-space indent");
        assert!(
            joined.contains(PLACEHOLDER),
            "Must contain placeholder character"
        );
        let count = joined.matches(PLACEHOLDER).count();
        assert_eq!(count, 10, "Must have exactly 10 placeholders for cols=10");
    }
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_render_mermaid_to_unicode_placeholders_success() {
    let src = "graph LR\n  A[Start] --> B[End]";
    let diag = render_mermaid_to_unicode_placeholders(src, Some(60));
    assert!(
        diag.is_some(),
        "Diagram should successfully rasterize to placeholders"
    );
    let d = diag.unwrap();
    assert!(d.cols >= 10);
    assert!(d.rows >= 3);
    assert_eq!(d.lines.len(), d.rows as usize);
    assert!(d.transmission.contains("U=1"));
}

#[test]
#[allow(deprecated)]
fn test_frame_unicode_placeholder_lines_alignment() {
    for cols in [5, 10, 20, 40, 80] {
        let lines = create_unicode_placeholder_lines(12345, cols, 3, 2);
        let framed = frame_unicode_placeholder_lines(lines, cols);
        assert!(framed.len() >= 5);
        let top_str = framed[0].spans[0].content.as_ref();
        let bot_str = framed[framed.len() - 2].spans[0].content.as_ref();
        let top_width = UnicodeWidthStr::width(top_str);
        let bot_width = UnicodeWidthStr::width(bot_str);
        assert_eq!(
            top_width, bot_width,
            "Top and bottom border widths must be equal for cols={cols}: top={top_width}, bot={bot_width}"
        );
    }
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_placeholder_lru_cache_hit() {
    let src = "graph LR\n  X[Alpha] --> Y[Beta]";
    let diag1 = render_mermaid_to_unicode_placeholders(src, Some(60)).expect("First render");
    let diag2 = render_mermaid_to_unicode_placeholders(src, Some(60)).expect("Second render");
    assert_eq!(
        diag1.image_id, diag2.image_id,
        "Cached diagram must retain stable image_id without generating new ones"
    );
    assert_eq!(diag1.cols, diag2.cols);
    assert_eq!(diag1.rows, diag2.rows);
    assert_eq!(diag1.lines.len(), diag2.lines.len());
}

#[test]
fn test_multichunk_transmission_spec_compliance() {
    // Generate payload larger than 4096 base64 bytes (> 3072 raw bytes)
    let large_bytes = vec![0xABu8; 6000];
    let seq = encode_kitty_unicode_transmission(&large_bytes, 777, 50, 25);
    let chunks: Vec<&str> = seq.split("\x1b\\").filter(|s| !s.is_empty()).collect();
    assert!(chunks.len() >= 2, "Must be split into at least 2 chunks");

    // Chunk 0 must specify a=T, f=100, i=777, c=50, r=25, U=1
    assert!(chunks[0].contains("a=T"));
    assert!(chunks[0].contains("i=777"));
    assert!(chunks[0].contains("m=1"));

    // Subsequent chunks must NOT repeat i= or a=
    for sub in &chunks[1..] {
        assert!(
            !sub.contains("i=777"),
            "Subsequent chunk must not repeat i= id"
        );
        assert!(
            !sub.contains("a=T"),
            "Subsequent chunk must not repeat a= action"
        );
        assert!(
            sub.contains("m="),
            "Subsequent chunk must specify m= chunk marker"
        );
    }
}

#[test]
fn test_diacritics_count_for_24bit_vs_32bit_id() {
    // Standard 24-bit ID: only 2 diacritics per cell (row, col)
    let id_24bit = 0x001234;
    let lines_24 = create_unicode_placeholder_lines(id_24bit, 1, 1, 0);
    let cell_24 = lines_24[0].spans[0].content.as_ref();
    let chars_24: Vec<char> = cell_24.chars().collect();
    assert_eq!(
        chars_24.len(),
        3,
        "24-bit ID must have exactly 1 base + 2 diacritics (row, col)"
    );
    assert_eq!(chars_24[0], PLACEHOLDER);

    // 32-bit ID (MSB > 0): 3 diacritics per cell (row, col, msb)
    let id_32bit = 0x02001234;
    let lines_32 = create_unicode_placeholder_lines(id_32bit, 1, 1, 0);
    let cell_32 = lines_32[0].spans[0].content.as_ref();
    let chars_32: Vec<char> = cell_32.chars().collect();
    assert_eq!(
        chars_32.len(),
        4,
        "32-bit ID with MSB > 0 must have 1 base + 3 diacritics"
    );
}

#[test]
fn test_is_kitty_supported_escape_hatch() {
    let _lock = crate::utils::lock_test_env();
    let prev_wm = std::env::var("WM_INLINE_DIAGRAMS").ok();
    let prev_mm = std::env::var("MM_INLINE_DIAGRAMS").ok();
    unsafe {
        std::env::remove_var("WM_INLINE_DIAGRAMS");
        std::env::remove_var("MM_INLINE_DIAGRAMS");
    }

    unsafe {
        std::env::set_var("WM_INLINE_DIAGRAMS", "0");
    }
    assert!(
        !is_kitty_supported(),
        "WM_INLINE_DIAGRAMS=0 must disable support regardless of other vars"
    );

    unsafe {
        std::env::set_var("WM_INLINE_DIAGRAMS", "1");
    }
    assert!(
        is_kitty_supported(),
        "WM_INLINE_DIAGRAMS=1 must force enable support"
    );

    unsafe {
        std::env::remove_var("WM_INLINE_DIAGRAMS");
        std::env::set_var("MM_INLINE_DIAGRAMS", "0");
    }
    assert!(
        !is_kitty_supported(),
        "MM_INLINE_DIAGRAMS=0 fallback must disable support"
    );

    unsafe {
        std::env::set_var("MM_INLINE_DIAGRAMS", "1");
    }
    assert!(
        is_kitty_supported(),
        "MM_INLINE_DIAGRAMS=1 fallback must force enable support"
    );

    unsafe {
        match prev_wm {
            Some(v) => std::env::set_var("WM_INLINE_DIAGRAMS", v),
            None => std::env::remove_var("WM_INLINE_DIAGRAMS"),
        }
        match prev_mm {
            Some(v) => std::env::set_var("MM_INLINE_DIAGRAMS", v),
            None => std::env::remove_var("MM_INLINE_DIAGRAMS"),
        }
    }
}

#[test]
fn test_parse_colorfgbg() {
    assert_eq!(parse_colorfgbg("15;0"), Some(ColorMode::Dark));
    assert_eq!(parse_colorfgbg("7;0"), Some(ColorMode::Dark));
    assert_eq!(parse_colorfgbg("0;7"), Some(ColorMode::Light));
    assert_eq!(parse_colorfgbg("0;15"), Some(ColorMode::Light));
    assert_eq!(parse_colorfgbg("15;default;0"), Some(ColorMode::Dark));
    assert_eq!(parse_colorfgbg("0;default;15"), Some(ColorMode::Light));
    assert_eq!(parse_colorfgbg("15;default"), Some(ColorMode::Dark));
    assert_eq!(parse_colorfgbg("0;default"), Some(ColorMode::Light));
    assert_eq!(parse_colorfgbg("dark"), Some(ColorMode::Dark));
    assert_eq!(parse_colorfgbg("DARK"), Some(ColorMode::Dark));
    assert_eq!(parse_colorfgbg("light"), Some(ColorMode::Light));
    assert_eq!(parse_colorfgbg("invalid"), None);
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_build_mermaid_theme_transparency_and_modes() {
    // Transparent dark theme
    let dark_transparent = build_mermaid_theme(DiagramTheme::Dark, DiagramBackground::Transparent);
    assert_eq!(
        dark_transparent.background, "#00000000",
        "Transparent background must be hex #00000000"
    );
    assert!(!dark_transparent.primary_text_color.is_empty());

    // Solid dark theme
    let dark_solid = build_mermaid_theme(DiagramTheme::Dark, DiagramBackground::Solid);
    assert_ne!(
        dark_solid.background, "#00000000",
        "Solid background must not be transparent"
    );

    // Transparent light theme
    let light_transparent =
        build_mermaid_theme(DiagramTheme::Light, DiagramBackground::Transparent);
    assert_eq!(light_transparent.background, "#00000000");

    // Solid light theme
    let light_solid = build_mermaid_theme(DiagramTheme::Light, DiagramBackground::Solid);
    assert_ne!(light_solid.background, "#00000000");
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_build_mermaid_theme_omarchy_mismatch_protection() {
    // Even if Omarchy has dark mode colors active on the current machine,
    // an explicit Light theme request must NOT use dark mode's navy background or light text.
    let light_solid = build_mermaid_theme(DiagramTheme::Light, DiagramBackground::Solid);
    assert_eq!(
        light_solid.background, "#ffffff",
        "Light solid background must be white, not dark navy from Omarchy"
    );
    assert_eq!(
        light_solid.primary_text_color, "#0f172a",
        "Light primary text color must be dark high-contrast (#0f172a)"
    );

    let light_trans = build_mermaid_theme(DiagramTheme::Light, DiagramBackground::Transparent);
    assert_eq!(light_trans.background, "#00000000");
    assert_eq!(
        light_trans.primary_text_color, "#0f172a",
        "Light transparent primary text color must be dark high-contrast"
    );
}

#[test]
fn test_compute_theme_fingerprint_isolation() {
    let fp_dark_trans =
        compute_theme_fingerprint(DiagramTheme::Dark, DiagramBackground::Transparent);
    let fp_dark_solid = compute_theme_fingerprint(DiagramTheme::Dark, DiagramBackground::Solid);
    let fp_light_trans =
        compute_theme_fingerprint(DiagramTheme::Light, DiagramBackground::Transparent);
    let fp_light_solid = compute_theme_fingerprint(DiagramTheme::Light, DiagramBackground::Solid);

    assert_ne!(fp_dark_trans, fp_dark_solid);
    assert_ne!(fp_dark_trans, fp_light_trans);
    assert_ne!(fp_light_trans, fp_light_solid);
    assert_ne!(fp_dark_solid, fp_light_solid);
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_render_mermaid_to_image_transparent_pixels() {
    let src = "graph LR\n  A[Foo] --> B[Bar]";
    let img = render_mermaid_to_image_with_options(
        src,
        1.0,
        DiagramTheme::Dark,
        DiagramBackground::Transparent,
    );
    assert!(img.is_some(), "Diagram should successfully render to image");
    let dynamic = img.unwrap();
    let rgba = dynamic.to_rgba8();

    // Corner pixel (0, 0) should be transparent (alpha == 0)
    let corner_pixel = rgba.get_pixel(0, 0);
    assert_eq!(
        corner_pixel[3], 0,
        "Corner pixel of transparent diagram must have alpha == 0, got rgba: {:?}",
        corner_pixel
    );

    // Render solid diagram and verify corner pixel is opaque
    let solid_img = render_mermaid_to_image_with_options(
        src,
        1.0,
        DiagramTheme::Dark,
        DiagramBackground::Solid,
    );
    assert!(solid_img.is_some());
    let solid_rgba = solid_img.unwrap().to_rgba8();
    let solid_corner = solid_rgba.get_pixel(0, 0);
    assert_eq!(
        solid_corner[3], 255,
        "Corner pixel of solid diagram must have alpha == 255, got rgba: {:?}",
        solid_corner
    );
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_diagram_cache_differentiation_by_theme_and_bg() {
    let src = "graph TD\n  Start --> Finish";
    let img_trans = render_mermaid_to_image_with_options(
        src,
        1.0,
        DiagramTheme::Dark,
        DiagramBackground::Transparent,
    )
    .unwrap();
    let img_solid = render_mermaid_to_image_with_options(
        src,
        1.0,
        DiagramTheme::Dark,
        DiagramBackground::Solid,
    )
    .unwrap();

    // Pixel (0, 0) alpha must differ between transparent and solid
    assert_eq!(img_trans.to_rgba8().get_pixel(0, 0)[3], 0);
    assert_eq!(img_solid.to_rgba8().get_pixel(0, 0)[3], 255);
}

#[test]
fn test_diagram_theme_and_bg_enums() {
    assert_eq!("auto".parse::<DiagramTheme>().unwrap(), DiagramTheme::Auto);
    assert_eq!("dark".parse::<DiagramTheme>().unwrap(), DiagramTheme::Dark);
    assert_eq!(
        "light".parse::<DiagramTheme>().unwrap(),
        DiagramTheme::Light
    );
    assert_eq!(
        "system".parse::<DiagramTheme>().unwrap(),
        DiagramTheme::Auto
    );
    assert!("invalid".parse::<DiagramTheme>().is_err());

    assert_eq!(
        "transparent".parse::<DiagramBackground>().unwrap(),
        DiagramBackground::Transparent
    );
    assert_eq!(
        "solid".parse::<DiagramBackground>().unwrap(),
        DiagramBackground::Solid
    );
    assert_eq!(
        "none".parse::<DiagramBackground>().unwrap(),
        DiagramBackground::Transparent
    );
    assert_eq!(
        "opaque".parse::<DiagramBackground>().unwrap(),
        DiagramBackground::Solid
    );
    assert!("invalid".parse::<DiagramBackground>().is_err());

    assert_eq!(DiagramTheme::Dark.to_string(), "dark");
    assert_eq!(DiagramBackground::Transparent.to_string(), "transparent");
}
