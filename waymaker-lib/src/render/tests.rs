use super::*;
use crate::action::{Action, NullActionExt};
use crate::message::RenderCommand;
use ratatui::layout::Rect;
use ratatui::widgets::BorderType;

#[test]
fn test_nav_hints_grid_height_and_columns() {
    use crate::config::UiConfig;
    let mut ui = UiConfig::default();
    assert!(ui.nav.hints);
    assert_eq!(ui.nav.hints_columns, 4);

    // In 4 columns:
    // Full hints (16 items) -> (16 + 4 - 1) / 4 = 4 rows
    assert_eq!(ui.nav_hints_height(NAV_HINTS.len()), 4);

    // Basic hints (8 items) -> (8 + 4 - 1) / 4 = 2 rows
    assert_eq!(ui.nav_hints_height(BASIC_NAV_HINTS.len()), 2);

    // If columns configured to 1 -> 1 row
    ui.nav.hints_columns = 1;
    assert_eq!(ui.nav_hints_height(NAV_HINTS.len()), 1);

    // If nav_hints disabled -> 0 rows
    ui.nav.hints = false;
    assert_eq!(ui.nav_hints_height(NAV_HINTS.len()), 0);
}

#[test]
fn test_footer_separator_render_with_junction() {
    let backend = ratatui::backend::TestBackend::new(20, 1);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let area = ratatui::layout::Rect::new(0, 0, 20, 1);
            // Junction at column 8
            render_footer_separator(
                frame,
                area,
                crate::config::HorizontalSeparator::Light,
                &[8],
                Some(BorderType::Plain),
                Style::default(),
            );
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    // Columns 0..7 should be '─'
    for x in 0..8 {
        assert_eq!(buffer.cell((x, 0)).unwrap().symbol(), "─");
    }
    // Column 8 should be '┴'
    assert_eq!(buffer.cell((8, 0)).unwrap().symbol(), "┴");
    // Columns 9..19 should be '─'
    for x in 9..20 {
        assert_eq!(buffer.cell((x, 0)).unwrap().symbol(), "─");
    }
}

#[test]
fn test_footer_height_calculation_with_separator() {
    use crate::config::{DisplayConfig, HorizontalSeparator};
    use crate::ui::DisplayUI;

    let mut config = DisplayConfig::default();
    config.content = Some(crate::utils::serde::StringOrVec::String(
        "[Enter] Jump • [Esc] Exit".to_string(),
    ));
    config.separator = HorizontalSeparator::Light;

    let footer_ui = DisplayUI::new(config);
    assert_eq!(footer_ui.content_height(), 1);
    assert_eq!(footer_ui.height(), 2);

    let has_footer_separator = footer_ui.config.separator != HorizontalSeparator::None;
    let base_footer_height = footer_ui.content_height();
    let effective_footer_height = if base_footer_height > 0 && has_footer_separator {
        base_footer_height + 1
    } else {
        base_footer_height
    };

    // Must be exactly 2 (1 separator row + 1 content row), avoiding any dead space
    assert_eq!(effective_footer_height, 2);

    // When separator is None, effective height is exactly 1 row
    let mut config_no_sep = DisplayConfig::default();
    config_no_sep.content = Some(crate::utils::serde::StringOrVec::String(
        "[Enter] Jump • [Esc] Exit".to_string(),
    ));
    config_no_sep.separator = HorizontalSeparator::None;
    let footer_no_sep = DisplayUI::new(config_no_sep);
    assert_eq!(footer_no_sep.content_height(), 1);
    assert_eq!(footer_no_sep.height(), 1);
}

#[test]
fn test_preview_border_resolution_from_config() {
    use crate::preview::previewer::Previewer;
    let (previewer, _tx) = Previewer::new(Default::default());
    let mut config = crate::config::PreviewConfig::default();
    config.border.color = Color::DarkGray;
    config.layout.push(crate::config::PreviewSetting {
        border: None,
        ..Default::default()
    });
    let preview_ui = PreviewUI::new(previewer.view(), config, [80, 25]);

    // p.border() should fall back to config.border when layout has no border override
    assert_eq!(preview_ui.border().color, Color::DarkGray);
}

#[test]
fn test_preview_border_color_override_live_input() {
    use crate::preview::previewer::Previewer;
    let (previewer, _tx) = Previewer::new(Default::default());
    let mut config = crate::config::PreviewConfig::default();
    config.border.sides = Some(ratatui::widgets::Borders::LEFT);
    config.border.color = Color::DarkGray;
    config.layout.push(crate::config::PreviewSetting {
        border: None,
        ..Default::default()
    });
    let mut preview_ui = PreviewUI::new(previewer.view(), config, [80, 25]);

    assert_eq!(preview_ui.effective_border_color(), Some(Color::DarkGray));

    // When live input is active, border override takes precedence
    preview_ui.set_border_color_override(Some(Color::Green));
    assert_eq!(preview_ui.effective_border_color(), Some(Color::Green));

    let block = preview_ui.make_block().expect("block should exist");
    let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 10, 5));
    ratatui::widgets::Widget::render(block, Rect::new(0, 0, 10, 5), &mut buffer);
    assert_eq!(buffer.cell((0, 0)).unwrap().fg, Color::Green);

    // Resetting returns back to DarkGray
    preview_ui.set_border_color_override(None);
    assert_eq!(preview_ui.effective_border_color(), Some(Color::DarkGray));
}

#[test]
fn test_preview_layout_gap_alignment_with_border_edge() {
    use crate::config::{Percentage, PreviewLayout, Side};
    let area = Rect::new(0, 0, 80, 24);

    // Side::Right with gap = 1: preview.x must equal gap.x (no phantom offset column!)
    let mut layout = PreviewLayout {
        side: Side::Right,
        percentage: Percentage::new(50),
        gap: 1,
        ..Default::default()
    };
    let [preview, picker, gap] = layout.split(area, None);
    assert_eq!(preview.x, 40);
    assert_eq!(picker.width, 40);
    assert_eq!(gap.x, 40);
    assert_eq!(gap.width, 1);
    assert_eq!(gap.height, 24);

    // Side::Left with gap = 1: gap.x must align with preview's rightmost border column
    layout.side = Side::Left;
    let [preview, picker, gap] = layout.split(area, None);
    assert_eq!(preview.x, 0);
    assert_eq!(preview.width, 40);
    assert_eq!(picker.x, 40);
    assert_eq!(gap.x, 39);
    assert_eq!(gap.width, 1);

    // Side::Top with gap = 1: gap.y must align with preview's bottom border row
    layout.side = Side::Top;
    let [preview, picker, gap] = layout.split(area, None);
    assert_eq!(preview.y, 0);
    assert_eq!(preview.height, 12);
    assert_eq!(picker.y, 12);
    assert_eq!(gap.y, 11);
    assert_eq!(gap.height, 1);

    // Side::Bottom with gap = 1: gap.y must align with preview's top border row
    layout.side = Side::Bottom;
    let [preview, picker, gap] = layout.split(area, None);
    assert_eq!(picker.height, 12);
    assert_eq!(preview.y, 12);
    assert_eq!(gap.y, 12);
    assert_eq!(gap.height, 1);

    // gap > 1: widened slot for counter badges is carved between the panes
    layout.side = Side::Right;
    layout.gap = 5;
    let [preview, picker, gap] = layout.split(area, None);
    assert_eq!(picker.width, 35);
    assert_eq!(gap.x, 35);
    assert_eq!(gap.width, 5);
    assert_eq!(preview.x, 40);
}

#[test]
fn test_preview_hover_shadow_overlays_vertical_border() {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    let area = Rect::new(0, 0, 80, 24);
    let layout = crate::config::PreviewLayout {
        side: crate::config::Side::Right,
        percentage: crate::config::Percentage::new(50),
        gap: 1,
        ..Default::default()
    };
    let [preview_area, _picker_area, gap_area] = layout.split(area, None);

    // Pre-render a vertical line at preview_area.x (representing the preview left border)
    terminal
        .draw(|frame| {
            let border_block = ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::LEFT)
                .border_style(ratatui::style::Style::default().fg(Color::DarkGray));
            frame.render_widget(border_block, preview_area);

            // Simulate hover shadow rendering over gap_area
            let buf = frame.buffer_mut();
            let shadow_style = Style::default().bg(Color::DarkGray);
            for y in gap_area.top()..gap_area.bottom() {
                for x in gap_area.left()..gap_area.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_char(' ');
                        cell.set_style(shadow_style);
                    }
                }
            }
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    // Column 40 (preview left border / gap) should have ' ' with bg(DarkGray)
    for y in 0..24 {
        let cell = buffer.cell((40, y)).unwrap();
        assert_eq!(cell.symbol(), " ");
        assert_eq!(cell.bg, Color::DarkGray);
    }
}

#[test]
fn test_apply_prompt_input() {
    // When not active, buffer is untouched
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "enter".to_string(),
        action: Action::Redraw,
    }];
    apply_prompt_input(&mut buffer, false);
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::KeyAction { .. }));

    // When active:
    // 1. Enter maps to Action::Accept
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "enter".to_string(),
        action: Action::Redraw,
    }];
    apply_prompt_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Accept)));

    // 2. Esc maps to Action::Cancel
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "esc".to_string(),
        action: Action::Quit(0),
    }];
    apply_prompt_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Cancel)));

    // 3. Backspace maps to Action::DeleteChar
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "backspace".to_string(),
        action: Action::Redraw,
    }];
    apply_prompt_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::DeleteChar)
    ));

    // 4. Regular characters pass through as Action(Char(c))
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('a'))];
    apply_prompt_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::Char('a'))
    ));
}

#[test]
fn test_apply_live_input() {
    // When not active, buffer is untouched
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "enter".to_string(),
        action: Action::Redraw,
    }];
    apply_live_input(&mut buffer, false);
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::KeyAction { .. }));

    // 1. Esc exits with Action::Cancel
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "esc".to_string(),
        action: Action::Quit(0),
    }];
    apply_live_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Cancel)));

    // 2. Consecutive characters are coalesced into a single Action::LiveInputKey batch
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Char('a')),
        RenderCommand::<NullActionExt>::Action(Action::Char('b')),
        RenderCommand::<NullActionExt>::Action(Action::Char('c')),
    ];
    apply_live_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    match &buffer[0] {
        RenderCommand::Action(Action::LiveInputKey(keys)) => {
            assert_eq!(keys, "'abc'");
        }
        other => panic!("expected LiveInputKey, got {:?}", other),
    }

    // 3. Characters followed by Enter coalesce into "'ls' 'Enter'"
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Char('l')),
        RenderCommand::<NullActionExt>::Action(Action::Char('s')),
        RenderCommand::<NullActionExt>::Action(Action::Accept),
    ];
    apply_live_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    match &buffer[0] {
        RenderCommand::Action(Action::LiveInputKey(keys)) => {
            assert_eq!(keys, "'ls' 'Enter'");
        }
        other => panic!("expected LiveInputKey, got {:?}", other),
    }

    // 4. Named keys normalize properly
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::DeleteChar),
        RenderCommand::<NullActionExt>::Action(Action::Up(1)),
        RenderCommand::<NullActionExt>::KeyAction {
            key: "ctrl-c".to_string(),
            action: Action::Redraw,
        },
    ];
    apply_live_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    match &buffer[0] {
        RenderCommand::Action(Action::LiveInputKey(keys)) => {
            assert_eq!(keys, "'BSpace' 'Up' 'C-c'");
        }
        other => panic!("expected LiveInputKey, got {:?}", other),
    }

    // 5. PreviewChange command is passed through and flushes batch
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Char('x')),
        RenderCommand::<NullActionExt>::PreviewChange,
    ];
    apply_live_input(&mut buffer, true);
    assert_eq!(buffer.len(), 2);
    match &buffer[0] {
        RenderCommand::Action(Action::LiveInputKey(keys)) => {
            assert_eq!(keys, "'x'");
        }
        other => panic!("expected LiveInputKey, got {:?}", other),
    }
    assert!(matches!(buffer[1], RenderCommand::PreviewChange));

    // 6. Space character alone maps to "'Space'"
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char(' '))];
    apply_live_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    match &buffer[0] {
        RenderCommand::Action(Action::LiveInputKey(keys)) => {
            assert_eq!(keys, "'Space'");
        }
        other => panic!("expected LiveInputKey, got {:?}", other),
    }

    // 7. Space between words maps to "'foo' 'Space' 'bar'"
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Char('f')),
        RenderCommand::<NullActionExt>::Action(Action::Char('o')),
        RenderCommand::<NullActionExt>::Action(Action::Char('o')),
        RenderCommand::<NullActionExt>::Action(Action::Char(' ')),
        RenderCommand::<NullActionExt>::Action(Action::Char('b')),
        RenderCommand::<NullActionExt>::Action(Action::Char('a')),
        RenderCommand::<NullActionExt>::Action(Action::Char('r')),
    ];
    apply_live_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    match &buffer[0] {
        RenderCommand::Action(Action::LiveInputKey(keys)) => {
            assert_eq!(keys, "'foo' 'Space' 'bar'");
        }
        other => panic!("expected LiveInputKey, got {:?}", other),
    }

    // 8. Tab, Delete, BackTab map to "'Tab' 'DC' 'BTab'"
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::KeyAction {
            key: "tab".to_string(),
            action: Action::Redraw,
        },
        RenderCommand::<NullActionExt>::Action(Action::DeleteNextChar),
        RenderCommand::<NullActionExt>::KeyAction {
            key: "backtab".to_string(),
            action: Action::Redraw,
        },
    ];
    apply_live_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    match &buffer[0] {
        RenderCommand::Action(Action::LiveInputKey(keys)) => {
            assert_eq!(keys, "'Tab' 'DC' 'BTab'");
        }
        other => panic!("expected LiveInputKey, got {:?}", other),
    }

    // 9. Special characters and semicolons are properly isolated and quoted
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Char('(')),
        RenderCommand::<NullActionExt>::Action(Action::Char(';')),
        RenderCommand::<NullActionExt>::Action(Action::Char(')')),
        RenderCommand::<NullActionExt>::Action(Action::Char('\'')),
    ];
    apply_live_input(&mut buffer, true);
    assert_eq!(buffer.len(), 1);
    match &buffer[0] {
        RenderCommand::Action(Action::LiveInputKey(keys)) => {
            assert_eq!(keys, r"'(' '\;' ')'\'''");
        }
        other => panic!("expected LiveInputKey, got {:?}", other),
    }
}

// #[cfg(test)]
// async fn send_every_second(tx: mpsc::UnboundedSender<RenderCommand>) {
//     let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

//     loop {
//         interval.tick().await;
//         if tx.send(RenderCommand::quit()).is_err() {
//             break;
//         }
//     }
// }

/// With the sort menu open, Esc closes it and is swallowed; another key closes it and its action
/// still runs.
/// Each letter in the sort menu picks its order (upper case reverses it).
#[test]
fn test_sort_menu_letters() {
    use crate::action::SortOrder::*;
    for (c, order) in [
        ('a', Alphabetical),
        ('A', AlphabeticalReverse),
        ('n', Natural),
        ('N', NaturalReverse),
        ('m', Modified),
        ('M', ModifiedReverse),
        ('b', Created),
        ('B', CreatedReverse),
        ('s', Size),
        ('S', SizeReverse),
        ('e', Extension),
        ('E', ExtensionReverse),
    ] {
        let mut active = true;
        let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char(c))];
        keys::apply_sort_menu(&mut buffer, &mut active);
        assert!(!active, "{c}");
        assert!(
            matches!(buffer.as_slice(), [RenderCommand::Action(Action::Sort(Some(o)))] if *o == order),
            "{c}: {buffer:?}"
        );
    }
}

#[test]
fn test_sort_menu_esc_is_swallowed_other_keys_pass_through() {
    let mut active = true;
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "esc".into(),
        action: Action::Down(1),
    }];
    keys::apply_sort_menu(&mut buffer, &mut active);
    assert!(!active);
    assert!(buffer.is_empty(), "{buffer:?}");

    let mut active = true;
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "x".into(),
        action: Action::Down(1),
    }];
    keys::apply_sort_menu(&mut buffer, &mut active);
    assert!(!active);
    assert!(
        matches!(buffer.as_slice(), [RenderCommand::Action(Action::Down(1))]),
        "{buffer:?}"
    );
}
