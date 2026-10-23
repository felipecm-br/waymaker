use super::*;
use crate::action::{Action, Actions, NullActionExt};
use crate::message::RenderCommand;
use ratatui::widgets::BorderType;
use std::collections::HashMap;

#[test]
fn test_apply_focus_binds_sequences() {
    use crate::action::NullActionExt;

    let mut focus_binds = HashMap::new();
    focus_binds.insert("gg".to_string(), Actions::from([Action::PreviewUp(0)]));
    focus_binds.insert("G".to_string(), Actions::from([Action::PreviewDown(0)]));
    focus_binds.insert("J".to_string(), Actions::from([Action::PreviewDown(1)]));
    focus_binds.insert("K".to_string(), Actions::from([Action::PreviewUp(1)]));
    focus_binds.insert("gb".to_string(), Actions::from([Action::Pos(-1)]));
    focus_binds.insert("gt".to_string(), Actions::from([Action::Pos(0)]));

    let mut pending = None;
    let mut sort_menu_active = false;

    // Test gg
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Char('g')),
        RenderCommand::<NullActionExt>::Action(Action::Char('g')),
    ];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewUp(0))
    ));
    assert_eq!(pending, None);

    // Test G
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('G'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewDown(0))
    ));
    assert_eq!(pending, None);

    // Test J
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('J'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewDown(1))
    ));
    assert_eq!(pending, None);

    // Test K
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('K'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewUp(1))
    ));
    assert_eq!(pending, None);

    // Test gt
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Char('g')),
        RenderCommand::<NullActionExt>::Action(Action::Char('t')),
    ];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Pos(0))));
    assert_eq!(pending, None);

    // Test gb
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Char('g')),
        RenderCommand::<NullActionExt>::Action(Action::Char('b')),
    ];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Pos(-1))));
    assert_eq!(pending, None);

    // Test single 'g' when 'g' is explicitly bound in focus_binds
    let mut custom_binds = HashMap::new();
    custom_binds.insert("g".to_string(), Actions::from([Action::Quit(0)]));
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('g'))];
    let mut custom_pending = None;
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &custom_binds,
        false,
        &mut custom_pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Quit(0))));
    assert_eq!(custom_pending, None);

    // Test ',' opens sort menu
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char(','))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::SortMenu)));
    assert!(sort_menu_active);

    // Test next key 'n' in sort menu dispatches Action::Sort(Natural) and closes menu
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('n'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::Sort(Some(crate::action::SortOrder::Natural)))
    ));
    assert!(!sort_menu_active);

    // Test next key 'b' in sort menu dispatches Action::Sort(Created) and closes menu
    sort_menu_active = true;
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('b'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::Sort(Some(crate::action::SortOrder::Created)))
    ));
    assert!(!sort_menu_active);

    // Test ',' then 'Esc' cancels without dispatching sort
    sort_menu_active = true;
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Quit(1))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 0);
    assert!(!sort_menu_active);
}

#[test]
fn test_apply_focus_binds_key_actions() {
    use crate::action::NullActionExt;

    let mut focus_binds = HashMap::new();
    focus_binds.insert("esc".to_string(), Actions::from([Action::Quit(1)]));
    focus_binds.insert("q".to_string(), Actions::from([Action::Quit(1)]));
    focus_binds.insert("/".to_string(), Actions::from([Action::FocusFilter]));
    focus_binds.insert(
        "backspace".to_string(),
        Actions::from([
            Action::ChDir("..".to_string()),
            Action::Cancel,
            Action::Reload("".to_string()),
            Action::FocusNav,
        ]),
    );

    let mut pending = None;
    let mut sort_menu_active = false;

    // 1. Esc in Focus::Results with "esc" = "Quit" in nav_binds must emit Action::Quit(1)
    // Works both with lowercase "esc" and crokey's capitalized "Esc"
    for esc_key in ["esc", "Esc"] {
        let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
            key: esc_key.to_string(),
            action: Action::ToggleFocus,
        }];
        apply_focus_binds(
            &mut buffer,
            Focus::Results,
            &focus_binds,
            false,
            &mut pending,
            &mut sort_menu_active,
            false,
        );
        assert_eq!(buffer.len(), 1);
        assert!(matches!(buffer[0], RenderCommand::Action(Action::Quit(1))));
    }

    // 2. Esc in Focus::Input must toggle focus to Results
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "Esc".to_string(),
        action: Action::ToggleFocus,
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Input,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::ToggleFocus)
    ));

    // 3. Backspace in Focus::Results must run custom nav_binds (case-insensitive)
    for bs_key in ["backspace", "Backspace"] {
        let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
            key: bs_key.to_string(),
            action: Action::DeleteChar,
        }];
        apply_focus_binds(
            &mut buffer,
            Focus::Results,
            &focus_binds,
            false,
            &mut pending,
            &mut sort_menu_active,
            false,
        );
        assert_eq!(buffer.len(), 4);
        assert!(matches!(buffer[0], RenderCommand::Action(Action::ChDir(_))));
        assert!(matches!(buffer[1], RenderCommand::Action(Action::Cancel)));
        assert!(matches!(
            buffer[2],
            RenderCommand::Action(Action::Reload(_))
        ));
        assert!(matches!(buffer[3], RenderCommand::Action(Action::FocusNav)));
    }

    // 4. Backspace in Focus::Input must delete char
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "Backspace".to_string(),
        action: Action::DeleteChar,
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Input,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::DeleteChar)
    ));

    // 5. Esc in Focus::Results WITHOUT "esc" in nav_binds must quit with 130 instead of trapping focus
    for esc_key in ["esc", "Esc"] {
        let empty_binds = HashMap::new();
        let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
            key: esc_key.to_string(),
            action: Action::ToggleFocus,
        }];
        apply_focus_binds(
            &mut buffer,
            Focus::Results,
            &empty_binds,
            false,
            &mut pending,
            &mut sort_menu_active,
            false,
        );
        assert_eq!(buffer.len(), 1);
        assert!(matches!(
            buffer[0],
            RenderCommand::Action(Action::Quit(130))
        ));
    }
}

#[test]
fn test_apply_focus_binds_esc_and_nav() {
    use crate::action::{Actions, NullActionExt};
    let mut focus_binds = std::collections::HashMap::new();
    focus_binds.insert("esc".to_string(), Actions::from([Action::Quit(0)]));
    focus_binds.insert("q".to_string(), Actions::from([Action::Quit(0)]));
    focus_binds.insert("/".to_string(), Actions::from([Action::FocusFilter]));
    focus_binds.insert("backspace".to_string(), Actions::from([Action::Pos(0)]));

    let mut pending = None;
    let mut sort_menu_active = false;

    // 1. In Input mode, ToggleFocus passes through
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::ToggleFocus)];
    apply_focus_binds(
        &mut buffer,
        Focus::Input,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::ToggleFocus)
    ));

    // 2. In Results mode, ToggleFocus is intercepted and replaced by nav_binds["esc"] (Quit)
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::ToggleFocus)];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Quit(0))));

    // 3. In Results mode, 'q' triggers Quit
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('q'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Quit(0))));

    // 4. In Results mode, '/' triggers FocusFilter
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('/'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::FocusFilter)
    ));

    // 5. In Results mode, DeleteChar triggers 'backspace' bind
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::DeleteChar)];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Pos(0))));
}

#[test]
fn test_apply_focus_binds_tab_reloadnext_no_multiplication() {
    use crate::action::{Actions, NullActionExt};
    let mut focus_binds = std::collections::HashMap::new();
    focus_binds.insert(
        "tab".to_string(),
        Actions::from([Action::Semantic("reloadnext".to_string())]),
    );

    let mut pending = None;
    let mut sort_menu_active = false;

    // 1. Traces surrounding a KeyAction for "Tab" should not duplicate the nav_bind
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Trace("@@reloadnext".to_string())),
        RenderCommand::<NullActionExt>::KeyAction {
            key: "Tab".to_string(),
            action: Action::Semantic("reloadnext".to_string()),
        },
        RenderCommand::<NullActionExt>::Action(Action::Trace(String::new())),
    ];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    let reloadnext_count = buffer
        .iter()
        .filter(|cmd| match cmd {
            RenderCommand::Action(Action::Semantic(s)) => s == "reloadnext",
            _ => false,
        })
        .count();
    assert_eq!(
        reloadnext_count, 1,
        "Tab in Focus::Results must trigger reloadnext exactly once"
    );

    // 2. Multiple consecutive KeyActions with same key (e.g. from multi-action bind)
    // must execute the nav_bind only once
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::KeyAction {
            key: "Tab".to_string(),
            action: Action::Toggle,
        },
        RenderCommand::<NullActionExt>::KeyAction {
            key: "Tab".to_string(),
            action: Action::Down(1),
        },
    ];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    let reloadnext_count = buffer
        .iter()
        .filter(|cmd| match cmd {
            RenderCommand::Action(Action::Semantic(s)) => s == "reloadnext",
            _ => false,
        })
        .count();
    assert_eq!(
        reloadnext_count, 1,
        "Multi-action Tab must not duplicate nav_bind execution"
    );

    // 3. Unmapped multi-action key in Focus::Results preserves all fallback actions
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::KeyAction {
            key: "alt-l".to_string(),
            action: Action::Cancel,
        },
        RenderCommand::<NullActionExt>::KeyAction {
            key: "alt-l".to_string(),
            action: Action::Pos(0),
        },
    ];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(
        buffer.len(),
        2,
        "Unmapped key must preserve all fallback actions"
    );
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Cancel)));
    assert!(matches!(buffer[1], RenderCommand::Action(Action::Pos(0))));

    // 4. In Focus::Input, Tab executes the action directly
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "Tab".to_string(),
        action: Action::Semantic("reloadnext".to_string()),
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Input,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(
        matches!(buffer[0], RenderCommand::Action(Action::Semantic(ref s)) if s == "reloadnext")
    );
}

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
fn test_preview_fullscreen_key_navigation() {
    use crate::action::NullActionExt;
    let focus_binds = std::collections::HashMap::new();
    let mut pending = None;
    let mut sort_menu_active = false;

    // Esc in Results when preview_fullscreen is true -> CyclePreview
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "esc".to_string(),
        action: Action::ToggleFocus,
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::CyclePreview)
    ));

    // Esc in Input when preview_fullscreen is true -> CyclePreview
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "esc".to_string(),
        action: Action::ToggleFocus,
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Input,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::CyclePreview)
    ));

    // ToggleFocus action directly when preview_fullscreen is true -> CyclePreview
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::ToggleFocus)];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::CyclePreview)
    ));

    // j / k when preview_fullscreen is true -> PreviewDown(3) / PreviewUp(3)
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "j".to_string(),
        action: Action::Down(1),
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewDown(3))
    ));

    // h / l when preview_fullscreen is true -> PreviewHScroll(-6) / PreviewHScroll(6)
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('h'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewHScroll(-6))
    ));

    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('l'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewHScroll(6))
    ));

    // d when preview_fullscreen is true -> PreviewHalfPageDown
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('d'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewHalfPageDown)
    ));

    // ctrl-d when preview_fullscreen is true -> PreviewHalfPageDown
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "ctrl-d".to_string(),
        action: Action::PreviewHalfPageDown,
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewHalfPageDown)
    ));

    // u and ctrl-u when preview_fullscreen is true -> PreviewHalfPageUp
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('u'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewHalfPageUp)
    ));

    // s when preview_fullscreen is true -> ToggleDiagram
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('s'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::ToggleDiagram)
    ));

    // enter when preview_fullscreen is true -> CyclePreview
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "enter".to_string(),
        action: Action::CyclePreview,
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::CyclePreview)
    ));

    // y when preview_fullscreen is true -> Accept
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "y".to_string(),
        action: Action::Char('y'),
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Accept)));

    // j / k in Focus::Input when preview_fullscreen is true -> PreviewDown(3) / PreviewUp(3)
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('j'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Input,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewDown(3))
    ));

    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "k".to_string(),
        action: Action::Char('k'),
    }];
    apply_focus_binds(
        &mut buffer,
        Focus::Input,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewUp(3))
    ));

    // Zoom keys when preview_fullscreen is true
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('+'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::DiagramZoomIn)
    ));

    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('-'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::DiagramZoomOut)
    ));

    // gg and G scroll jumps when preview_fullscreen is true
    let mut buffer = vec![
        RenderCommand::<NullActionExt>::Action(Action::Char('g')),
        RenderCommand::<NullActionExt>::Action(Action::Char('g')),
    ];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewUp(0))
    ));

    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('G'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewDown(0))
    ));
}

#[test]
fn test_apply_focus_binds_diagram_mode_navigation_and_zoom_reset() {
    let focus_binds = HashMap::new();
    let mut pending = None;
    let mut sort_menu_active = false;

    // In diagram mode (even non-fullscreen preview):
    // 1. Arrow keys pan the diagram
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "up".to_string(),
        action: Action::Up(1),
    }];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        true, // preview_diagram_mode
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewUp(3))
    ));

    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "down".to_string(),
        action: Action::Down(1),
    }];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewDown(3))
    ));

    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "left".to_string(),
        action: Action::BackwardChar,
    }];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewHScroll(-6))
    ));

    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "right".to_string(),
        action: Action::ForwardChar,
    }];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewHScroll(6))
    ));

    // 2. '0' resets zoom
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('0'))];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::DiagramResetZoom)
    ));

    // 3. 'z' does NOT reset zoom
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('z'))];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        true,
    );
    assert!(!matches!(
        buffer[0],
        RenderCommand::Action(Action::DiagramResetZoom)
    ));

    // 4. Action::Up(1) and Action::Down(1) without KeyAction wrapper in diagram mode
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Up(1))];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewUp(3))
    ));

    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Down(1))];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewDown(3))
    ));

    // 5. Fallback ctrl-shift-j and ctrl-shift-k preview scrolling
    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "ctrl-shift-j".to_string(),
        action: Action::PreviewDown(5),
    }];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewDown(5))
    ));

    let mut buffer = vec![RenderCommand::<NullActionExt>::KeyAction {
        key: "ctrl-shift-k".to_string(),
        action: Action::PreviewUp(5),
    }];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        &focus_binds,
        false,
        &mut pending,
        &mut sort_menu_active,
        false,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::PreviewUp(5))
    ));
}

#[test]
fn test_get_nav_bind_question_mark() {
    let mut focus_binds = std::collections::HashMap::new();
    focus_binds.insert(
        "?".to_string(),
        crate::action::Actions::from(vec![Action::SwitchPreview(Some(1))]),
    );

    // Key "?" matches directly
    let res = get_nav_bind(&focus_binds, "?");
    assert!(res.is_some());
    assert_eq!(res.unwrap()[0], Action::SwitchPreview(Some(1)));

    // Key "shift-?" cross-matches to "?"
    let res_shift = get_nav_bind(&focus_binds, "shift-?");
    assert!(res_shift.is_some());
    assert_eq!(res_shift.unwrap()[0], Action::SwitchPreview(Some(1)));

    // And vice-versa: binding "shift-?" matches when key is "?"
    let mut focus_binds_shift = std::collections::HashMap::new();
    focus_binds_shift.insert(
        "shift-?".to_string(),
        crate::action::Actions::from(vec![Action::Help("".into())]),
    );
    let res_rev = get_nav_bind(&focus_binds_shift, "?");
    assert!(res_rev.is_some());
    assert_eq!(res_rev.unwrap()[0], Action::Help("".into()));
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
