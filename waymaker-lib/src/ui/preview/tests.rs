use super::*;

#[test]
fn test_calc_scrollbar_compatibility() {
    let tests = vec![
        ("page size greater than list size", 5, 10, 0, 20, 0, 20),
        ("page size matches list size", 10, 10, 0, 20, 0, 20),
        ("page size half of list size", 10, 5, 0, 20, 0, 10),
        (
            "page size half of list size at scroll end",
            10,
            5,
            5,
            20,
            10,
            10,
        ),
        (
            "page size third of list size having scrolled half the way",
            15,
            5,
            5,
            21,
            7,
            7,
        ),
        (
            "page size third of list size having scrolled the full way",
            15,
            5,
            10,
            21,
            14,
            7,
        ),
        (
            "page size third of list size having scrolled by one",
            15,
            5,
            1,
            21,
            2,
            7,
        ),
        (
            "page size third of list size having scrolled up from the bottom by one",
            15,
            5,
            9,
            21,
            12,
            7,
        ),
    ];

    for (name, list_size, page_size, position, scroll_area_size, expected_start, expected_height) in
        tests
    {
        let (start, height) = calc_scrollbar(list_size, page_size, position, scroll_area_size);
        assert_eq!(
            start, expected_start,
            "Test '{}' failed: expected start {}, got {}",
            name, expected_start, start
        );
        assert_eq!(
            height, expected_height,
            "Test '{}' failed: expected height {}, got {}",
            name, expected_height, height
        );
    }
}

#[test]
fn test_render_scrollbar_draws_half_blocks_when_enabled() {
    use crate::preview::previewer::Previewer;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::text::Text;

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut config = PreviewConfig::default();
    config.scrollbar = true;
    config.show = crate::config::ShowCondition::Bool(true);

    let (previewer, _tx) = Previewer::new(Default::default());
    let text: Text<'static> = (1..=30)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n")
        .into();
    previewer.set_string(text);

    let mut ui = PreviewUI::new(previewer.view(), config, [40, 10]);
    ui.update_dimensions(&Rect::new(0, 0, 40, 10));

    terminal
        .draw(|f| {
            ui.render_scrollbar(f, Rect::new(0, 0, 40, 10));
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let right_x = 39;
    let mut found_thumb = false;
    for y in 0..10 {
        if buf.cell((right_x, y)).map(|c| c.symbol()) == Some("▐") {
            found_thumb = true;
            break;
        }
    }
    assert!(
        found_thumb,
        "Scrollbar thumb '▐' should be rendered on right edge"
    );
}

#[test]
fn test_render_scrollbar_disabled_by_default() {
    use crate::preview::previewer::Previewer;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::text::Text;

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    let config = PreviewConfig::default();
    assert!(
        !config.scrollbar,
        "Preview scrollbar must be disabled by default"
    );

    let (previewer, _tx) = Previewer::new(Default::default());
    let text: Text<'static> = (1..=30)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n")
        .into();
    previewer.set_string(text);

    let mut ui = PreviewUI::new(previewer.view(), config, [40, 10]);
    ui.update_dimensions(&Rect::new(0, 0, 40, 10));

    terminal
        .draw(|f| {
            ui.render_scrollbar(f, Rect::new(0, 0, 40, 10));
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    for y in 0..10 {
        for x in 0..40 {
            assert_ne!(buf.cell((x, y)).map(|c| c.symbol()), Some("▐"));
        }
    }
}

#[test]
fn test_preview_get_image_state() {
    let _env_lock = crate::utils::lock_test_env();
    use crate::preview::previewer::Previewer;
    let config = PreviewConfig {
        media: crate::config::PreviewMediaConfig {
            active: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (previewer, _tx) = Previewer::new(Default::default());
    let img = image::DynamicImage::new_rgb8(100, 100);
    previewer.set_image(img.clone());

    let mut ui = PreviewUI::new(previewer.view(), config, [40, 10]);
    let state = ui.get_image_state();
    assert!(
        state.is_some(),
        "State must be immediately available synchronously"
    );

    // Test Zoom In (2.0x magnification)
    ui.zoom = 2.0;
    ui.view
        .image_id
        .fetch_add(1, std::sync::atomic::Ordering::Release);
    let state_zoomed_in = ui.get_image_state();
    assert!(state_zoomed_in.is_some(), "State must be valid for zoom in");

    // Test Zoom Out (0.5x scaling)
    ui.zoom = 0.5;
    ui.view
        .image_id
        .fetch_add(1, std::sync::atomic::Ordering::Release);
    let state_zoomed_out = ui.get_image_state();
    assert!(
        state_zoomed_out.is_some(),
        "State must be valid for zoom out"
    );

    // Test dimension update (e.g. entering fullscreen preview)
    ui.update_dimensions(&Rect::new(0, 0, 80, 24));
    let state_resized = ui.get_image_state();
    assert!(
        state_resized.is_some(),
        "State must be updated for new area"
    );

    // Test Panning when zoomed in
    ui.zoom = 5.0;
    ui.down(5);
    assert_eq!(ui.pan_y, 5);
    let state_panned_down = ui.get_image_state();
    assert!(
        state_panned_down.is_some(),
        "State must update on vertical pan down"
    );

    ui.scroll(true, 10);
    assert_eq!(ui.pan_x, 10);
    let state_panned_right = ui.get_image_state();
    assert!(
        state_panned_right.is_some(),
        "State must update on horizontal pan right"
    );

    ui.up(2);
    assert_eq!(ui.pan_y, 3);
    ui.scroll(true, -4);
    assert_eq!(ui.pan_x, 6);

    ui.reset_scroll();
    assert_eq!(ui.pan_x, 0);
    assert_eq!(ui.pan_y, 0);

    // Test clearing image: get_image_state must become None immediately
    previewer.clear_image();
    assert!(
        ui.get_image_state().is_none(),
        "Image state must become None when image is cleared"
    );

    // Test title change resets image state
    previewer.set_image(img);
    assert!(ui.get_image_state().is_some());
    ui.set_title(Some("other_file.rs".to_string()));
    assert!(
        ui.image_state.is_none(),
        "Title change must clear image_state"
    );
}

#[test]
#[cfg(feature = "diagram-images")]
fn test_diagram_placeholder_lines_generation_and_panning() {
    let _env_lock = crate::utils::lock_test_env();
    use crate::preview::previewer::Previewer;
    let config = PreviewConfig {
        media: crate::config::PreviewMediaConfig {
            active: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (previewer, _tx) = Previewer::new(Default::default());
    let mmd_src = "graph TD\n    A[Start] --> B[Finish]\n".to_string();
    if let Ok(mut g) = previewer.view().diagram_sources.lock() {
        *g = vec![mmd_src];
    }

    let mut ui = PreviewUI::new(previewer.view(), config, [60, 20]);
    ui.show_diagram = true;
    assert!(ui.is_diagram_mode());
    assert_eq!(ui.diagram_scale(), 1.0);
    assert_eq!(ui.diagram_pan(), (0, 0));

    let area = Rect::new(0, 0, 60, 20);
    let lines_opt = ui.get_diagram_placeholder_lines(area);
    assert!(
        lines_opt.is_some(),
        "Diagram placeholder lines should be generated"
    );
    let lines = lines_opt.unwrap();
    assert_eq!(lines.len(), 20, "Lines count must match viewport height");

    // Inspect that lines contain Kitty Unicode Placeholder
    let has_placeholder = lines.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains(crate::utils::mermaid::PLACEHOLDER))
    });
    assert!(
        has_placeholder,
        "Rendered lines must contain Kitty Unicode Placeholders"
    );

    let initial_trans = ui.last_placeholder_transmission;
    assert!(
        initial_trans.is_some(),
        "Transmission parameters must be recorded"
    );
    let (_, _init_cols, _init_rows, init_id) = initial_trans.unwrap();

    // 1. Pan vertically and horizontally: verify ZERO re-transmissions (same image_id)
    ui.zoom = 2.0; // Zoom in to allow panning
    let _lines_zoomed = ui
        .get_diagram_placeholder_lines(area)
        .expect("Zoomed lines");
    let zoomed_trans = ui.last_placeholder_transmission.unwrap();
    assert_ne!(
        zoomed_trans.3, init_id,
        "Zoom change must allocate a new transmission ID"
    );

    let zoom_id = zoomed_trans.3;

    // Panning with pan_diagram_y / pan_diagram_x
    ui.pan_diagram_y(5);
    ui.pan_diagram_x(8);
    assert_eq!(ui.diagram_pan(), (8, 5));

    let lines_panned = ui
        .get_diagram_placeholder_lines(area)
        .expect("Panned lines");
    assert_eq!(lines_panned.len(), 20);

    let panned_trans = ui.last_placeholder_transmission.unwrap();
    assert_eq!(
        panned_trans.3, zoom_id,
        "CRITICAL: Panning must NEVER retransmit image across PTY — image_id must remain identical"
    );
    assert_eq!(
        panned_trans.1, zoomed_trans.1,
        "Columns must remain unchanged on pan"
    );
    assert_eq!(
        panned_trans.2, zoomed_trans.2,
        "Rows must remain unchanged on pan"
    );

    // 2. Reset diagram pan
    ui.reset_diagram_pan();
    assert_eq!(ui.diagram_pan(), (0, 0));
    assert_eq!(ui.diagram_scale(), 1.0);

    // 3. Negative pan protection (must clamp to 0 and NOT underflow u32 to max_pan)
    ui.zoom = 2.0;
    let _ = ui.get_diagram_placeholder_lines(area);
    ui.pan_x = -15;
    ui.pan_y = -30;
    let _ = ui.get_diagram_placeholder_lines(area);
    assert_eq!(
        ui.diagram_pan(),
        (0, 0),
        "Negative pan coordinates must safely clamp to 0 without u32 underflow"
    );

    // 4. Document offset preservation in diagram mode
    ui.offset = 77;
    ui.down(4);
    assert_eq!(
        ui.offset, 77,
        "Down action in diagram mode must NOT alter document text offset"
    );
    ui.up(2);
    assert_eq!(
        ui.offset, 77,
        "Up action in diagram mode must NOT alter document text offset"
    );
    ui.scroll_end();
    assert_eq!(
        ui.offset, 77,
        "scroll_end in diagram mode must NOT alter document text offset"
    );
    ui.reset_scroll();
    assert_eq!(
        ui.offset, 77,
        "reset_scroll in diagram mode must NOT alter document text offset"
    );

    // 5. Exiting diagram mode restores document offset handling
    ui.show_diagram = false;
    assert!(!ui.is_diagram_mode());
    ui.offset = 10;
    ui.up(3);
    assert_eq!(
        ui.offset, 7,
        "Normal mode up action must adjust text offset"
    );
}

#[test]
fn test_toggle_diagram_nearest_focus() {
    use crate::preview::previewer::Previewer;
    let (previewer, _tx) = Previewer::new(Default::default());
    let mut ui = PreviewUI::new(previewer.view(), PreviewConfig::default(), [80, 25]);
    let area = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 25,
    };
    ui.update_dimensions(&area);

    // Populate diagram offsets: diagram 0 at line 10, diagram 1 at line 60, diagram 2 at line 150
    *ui.view.diagram_offsets.lock().unwrap() = vec![10, 60, 150];
    *ui.view.diagram_sources.lock().unwrap() = vec![
        "graph TD\n  A --> B".to_string(),
        "graph LR\n  C --> D".to_string(),
        "graph TD\n  E --> F".to_string(),
    ];

    // Case 1: reading position is at line 55 (viewport height is 25, so lines 55..80 visible)
    // Diagram 1 is at line 60 (inside viewport!), so toggling diagram focuses diagram index 1
    ui.offset = 55;
    ui.toggle_diagram();
    assert!(ui.show_diagram);
    assert_eq!(
        ui.view
            .current_diagram_idx
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "Diagram 1 at line 60 is inside viewport 55..80 and must be focused"
    );
    assert_eq!(ui.diagram_pan(), (0, 0));
    assert_eq!(ui.diagram_scale(), 1.0);

    // Toggle off
    ui.toggle_diagram();
    assert!(!ui.show_diagram);

    // Case 2: reading position scrolled down to line 140
    // Diagram 2 at line 150 is closest (inside viewport 140..165)
    ui.offset = 140;
    ui.toggle_diagram();
    assert!(ui.show_diagram);
    assert_eq!(
        ui.view
            .current_diagram_idx
            .load(std::sync::atomic::Ordering::Relaxed),
        2,
        "Diagram 2 at line 150 must be focused when reading around line 140"
    );
    ui.toggle_diagram();

    // Case 3: reading position at top (line 0)
    // Diagram 0 at line 10 is inside viewport 0..25
    ui.offset = 0;
    ui.toggle_diagram();
    assert!(ui.show_diagram);
    assert_eq!(
        ui.view
            .current_diagram_idx
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "Diagram 0 at line 10 must be focused when reading at line 0"
    );
    ui.toggle_diagram();

    // Case 4: An off-screen diagram is just 2 lines above viewport (line 48 vs cur_scroll 50),
    // while an on-screen diagram is at line 60 (10 lines down, inside viewport 50..75).
    // The visible on-screen diagram (index 1) MUST be chosen over the off-screen diagram (index 0).
    *ui.view.diagram_offsets.lock().unwrap() = vec![48, 60, 150];
    ui.offset = 50;
    ui.toggle_diagram();
    assert!(ui.show_diagram);
    assert_eq!(
        ui.view
            .current_diagram_idx
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "Visible diagram at line 60 must beat off-screen diagram at line 48 even though 48 is close to viewport top"
    );
    ui.toggle_diagram();

    // Case 5: Multiple diagrams visible in viewport (line 55 and line 70, viewport 50..75).
    // The one closest to the reading position (top of viewport, line 55) must be chosen.
    *ui.view.diagram_offsets.lock().unwrap() = vec![10, 55, 70, 150];
    *ui.view.diagram_sources.lock().unwrap() = vec![
        "graph TD\n  A --> B".to_string(),
        "graph LR\n  C --> D".to_string(),
        "graph TD\n  E --> F".to_string(),
        "graph LR\n  G --> H".to_string(),
    ];
    ui.offset = 50;
    ui.toggle_diagram();
    assert!(ui.show_diagram);
    assert_eq!(
        ui.view
            .current_diagram_idx
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "When multiple diagrams are visible, the top one (line 55) must be chosen"
    );
}

#[test]
fn test_diagram_counter_spans() {
    use crate::preview::previewer::Previewer;
    let (previewer, _tx) = Previewer::new(Default::default());
    let mut ui = PreviewUI::new(previewer.view(), PreviewConfig::default(), [80, 25]);

    // Not in diagram mode -> None
    assert!(ui.diagram_counter_spans().is_none());

    *ui.view.diagram_sources.lock().unwrap() = vec![
        "graph TD\n  A --> B".to_string(),
        "graph LR\n  C --> D".to_string(),
        "graph TD\n  E --> F".to_string(),
    ];
    ui.show_diagram = true;
    ui.view
        .current_diagram_idx
        .store(1, std::sync::atomic::Ordering::Relaxed);

    let spans = ui
        .diagram_counter_spans()
        .expect("Counter spans in diagram mode");
    let full_text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(full_text, " [2/3] ");

    // With zoom at 1.5
    ui.zoom = 1.5;
    let spans_zoomed = ui
        .diagram_counter_spans()
        .expect("Counter spans when zoomed");
    let zoomed_text: String = spans_zoomed.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(zoomed_text, " [2/3 · 150%] ");
}

#[test]
fn test_preview_tail() {
    use crate::preview::previewer::Previewer;
    let (previewer, _tx) = Previewer::new(Default::default());
    let text: ratatui::text::Text<'static> = (1..=30)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n")
        .into();
    previewer.set_string(text);

    let mut config = PreviewConfig::default();
    config.initial.tail = true;
    let mut ui = PreviewUI::new(previewer.view(), config, [40, 10]);
    ui.update_dimensions(&Rect::new(0, 0, 40, 10));

    let _ = ui.make_preview();
    assert_eq!(
        ui.offset, 20,
        "Initial tail offset should position at bottom (30 - 10 = 20)"
    );
    assert!(ui.attained_target);

    // Scrolling up moves reading position backwards
    ui.up(1);
    assert_eq!(
        ui.offset, 19,
        "Scrolling up by 1 line decreases offset to 19"
    );

    // Scrolling down moves back towards tail
    ui.down(1);
    assert_eq!(ui.offset, 20, "Scrolling down returns to tail offset 20");

    // Switching item resets target and returns to tail on next preview render
    ui.set_target(None);
    assert_eq!(ui.offset, 0);
    assert!(!ui.attained_target);
    let _ = ui.make_preview();
    assert_eq!(ui.offset, 20);
    assert!(ui.attained_target);

    // When buffer grows from 30 to 50 lines while at tail:
    let new_text: ratatui::text::Text<'static> = (0..50)
        .map(|i| format!("Line {}", i))
        .collect::<Vec<_>>()
        .join("\n")
        .into();
    previewer.set_string(new_text);
    let _ = ui.make_preview();
    assert_eq!(
        ui.offset, 40,
        "Tail follow should automatically track new bottom (50 - 10 = 40)"
    );

    // If user scrolls up into history, growth should not yank away from reading position
    ui.up(5);
    assert_eq!(ui.offset, 35);
    let even_more_text: ratatui::text::Text<'static> = (0..60)
        .map(|i| format!("Line {}", i))
        .collect::<Vec<_>>()
        .join("\n")
        .into();
    previewer.set_string(even_more_text);
    let _ = ui.make_preview();
    assert_eq!(
        ui.offset, 35,
        "Should preserve reading scroll position when user scrolled up"
    );

    // scroll_end explicitly snaps back to tail
    ui.scroll_end();
    assert_eq!(
        ui.offset, 50,
        "scroll_end returns directly to bottom (60 - 10 = 50)"
    );
}

#[test]
fn test_preview_poll_interval_config() {
    use crate::preview::previewer::Previewer;
    let (previewer, _tx) = Previewer::new(Default::default());

    // Default has no poll interval
    let config = PreviewConfig::default();
    let ui = PreviewUI::new(previewer.view(), config, [40, 10]);
    assert_eq!(ui.poll_interval(), None);

    // Global poll_ms on PreviewConfig
    let mut config = PreviewConfig::default();
    config.poll_ms = Some(500);
    let ui = PreviewUI::new(previewer.view(), config, [40, 10]);
    assert_eq!(
        ui.poll_interval(),
        Some(std::time::Duration::from_millis(500))
    );

    // Layout-specific override
    let mut config = PreviewConfig::default();
    config.poll_ms = Some(500);
    let mut setting = crate::config::PreviewSetting::default();
    setting.poll_ms = Some(250);
    config.layout.push(setting);
    let ui = PreviewUI::new(previewer.view(), config, [40, 10]);
    assert_eq!(
        ui.poll_interval(),
        Some(std::time::Duration::from_millis(250))
    );
}
