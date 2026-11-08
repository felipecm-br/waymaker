//! Navigation-mode key resolution in the event loop (`EventLoop::resolve_nav`).
//!
//! These tests began as unit tests of the render loop's nav-key rewrite. `apply_focus_binds`
//! and `apply_focus_binds_mode` keep the old signatures but run one input event through the
//! event loop, then apply what the render loop still does: the sort menu takes the next key, and
//! applying `SortMenu` opens the menu.

use super::*;
use crate::action::{Action, Actions, NullActionExt};
use crate::input_state::RenderView;
use crate::message::RenderCommand;
use crate::render::Focus;
use crate::render::keys::get_nav_bind;
use std::collections::HashMap;

fn apply_focus_binds<A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    initial_focus: Focus,
    focus_binds: &HashMap<String, Actions<NullActionExt>>,
    overlay_active: bool,
    pending_nav_key: &mut Option<char>,
    sort_menu_active: &mut bool,
    preview_fullscreen: bool,
) {
    apply_focus_binds_mode(
        buffer,
        initial_focus,
        focus_binds,
        &HashMap::new(),
        overlay_active,
        pending_nav_key,
        sort_menu_active,
        preview_fullscreen,
        false,
    );
}

#[allow(clippy::too_many_arguments)]
fn apply_focus_binds_mode<A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    initial_focus: Focus,
    focus_binds: &HashMap<String, Actions<NullActionExt>>,
    preview_binds: &HashMap<String, Actions<NullActionExt>>,
    overlay_active: bool,
    pending_nav_key: &mut Option<char>,
    sort_menu_active: &mut bool,
    preview_fullscreen: bool,
    preview_diagram_mode: bool,
) {
    let mut event_loop = EventLoop::<A>::new();
    event_loop.set_nav(&crate::config::NavConfig {
        active: true,
        binds: focus_binds.clone(),
        preview_binds: preview_binds.clone(),
        ..Default::default()
    });
    event_loop.input.set_focus(initial_focus);
    event_loop.input.publish(
        RenderView {
            preview_fullscreen,
            preview_diagram: preview_diagram_mode,
            box_takes_keys: overlay_active,
            sort_menu: *sort_menu_active,
        },
        0,
    );
    event_loop.pending_nav_key = *pending_nav_key;
    let mut out = event_loop.resolve_nav(std::mem::take(buffer));
    *pending_nav_key = event_loop.pending_nav_key;

    // The render loop: the open sort menu takes the key, then the actions are applied.
    crate::render::keys::apply_sort_menu(&mut out, sort_menu_active);
    for cmd in &mut out {
        if let RenderCommand::KeyAction { action, .. } = cmd {
            *cmd = RenderCommand::Action(action.clone());
        }
        if matches!(cmd, RenderCommand::Action(Action::SortMenu)) {
            *sort_menu_active = !*sort_menu_active;
        }
    }
    *buffer = out;
}

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

    assert_focus_binds_sort_menu(&focus_binds, &mut pending, &mut sort_menu_active);
}

fn assert_focus_binds_sort_menu(
    focus_binds: &HashMap<String, Actions<NullActionExt>>,
    pending: &mut Option<char>,
    sort_menu_active: &mut bool,
) {
    // Test ',' opens sort menu
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char(','))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        focus_binds,
        false,
        pending,
        sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::SortMenu)));
    assert!(*sort_menu_active);

    // Test next key 'n' in sort menu dispatches Action::Sort(Natural) and closes menu
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('n'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        focus_binds,
        false,
        pending,
        sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::Sort(Some(crate::action::SortOrder::Natural)))
    ));
    assert!(!*sort_menu_active);

    // Test next key 'b' in sort menu dispatches Action::Sort(Created) and closes menu
    *sort_menu_active = true;
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('b'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        focus_binds,
        false,
        pending,
        sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(
        buffer[0],
        RenderCommand::Action(Action::Sort(Some(crate::action::SortOrder::Created)))
    ));
    assert!(!*sort_menu_active);

    // Test ',' then 'Esc' cancels without dispatching sort
    *sort_menu_active = true;
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Quit(1))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        focus_binds,
        false,
        pending,
        sort_menu_active,
        false,
    );
    assert_eq!(buffer.len(), 0);
    assert!(!*sort_menu_active);
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

    assert_preview_fullscreen_diagram_and_jumps(&focus_binds, &mut pending, &mut sort_menu_active);
}

fn assert_preview_fullscreen_diagram_and_jumps(
    focus_binds: &HashMap<String, Actions<NullActionExt>>,
    pending: &mut Option<char>,
    sort_menu_active: &mut bool,
) {
    // s when preview_fullscreen is true -> ToggleDiagram
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('s'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Results,
        focus_binds,
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        false,
        pending,
        sort_menu_active,
        true,
    );
    assert_eq!(buffer.len(), 1);
    assert!(matches!(buffer[0], RenderCommand::Action(Action::Accept)));

    // j / k in Focus::Input when preview_fullscreen is true -> PreviewDown(3) / PreviewUp(3)
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('j'))];
    apply_focus_binds(
        &mut buffer,
        Focus::Input,
        focus_binds,
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        false,
        pending,
        sort_menu_active,
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
        &HashMap::new(),
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
        &HashMap::new(),
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
        &HashMap::new(),
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
        &HashMap::new(),
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

    assert_diagram_zoom_reset_and_fallback_scroll(
        &focus_binds,
        &mut pending,
        &mut sort_menu_active,
    );
}

fn assert_diagram_zoom_reset_and_fallback_scroll(
    focus_binds: &HashMap<String, Actions<NullActionExt>>,
    pending: &mut Option<char>,
    sort_menu_active: &mut bool,
) {
    // 2. '0' resets zoom
    let mut buffer = vec![RenderCommand::<NullActionExt>::Action(Action::Char('0'))];
    apply_focus_binds_mode(
        &mut buffer,
        Focus::Results,
        focus_binds,
        &HashMap::new(),
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        &HashMap::new(),
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        &HashMap::new(),
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        &HashMap::new(),
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        &HashMap::new(),
        false,
        pending,
        sort_menu_active,
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
        focus_binds,
        &HashMap::new(),
        false,
        pending,
        sort_menu_active,
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

/// The nav and preview keymaps the event loop builds for `[nav.binds]` = `focus_binds`.
fn keymaps(
    focus_binds: &HashMap<String, Actions<NullActionExt>>,
) -> (
    crate::render::nav_keymap::NavKeymap,
    crate::render::nav_keymap::NavKeymap,
) {
    let nav = crate::config::NavConfig {
        binds: focus_binds.clone(),
        ..Default::default()
    };
    (
        crate::render::nav_keymap::NavKeymap::from_strings(&nav.effective_binds()),
        crate::render::nav_keymap::NavKeymap::from_strings(&nav.effective_preview_binds()),
    )
}

/// Runs one key through `process_results_nav_key` and returns what it emitted.
fn nav_key(
    key: &str,
    focus_binds: &HashMap<String, Actions<NullActionExt>>,
    preview_fullscreen: bool,
    preview_diagram_mode: bool,
) -> (Vec<Action<NullActionExt>>, Focus) {
    let mut pending = None;
    let mut focus = Focus::Results;
    let mut out = Vec::new();
    let (keymap, preview) = keymaps(focus_binds);
    crate::render::keys::process_results_nav_key::<NullActionExt>(
        key,
        None,
        &keymap,
        &preview,
        &mut pending,
        &mut focus,
        &mut out,
        preview_fullscreen,
        preview_diagram_mode,
    );
    let actions = out
        .into_iter()
        .filter_map(|c| match c {
            RenderCommand::Action(a) => Some(a),
            _ => None,
        })
        .collect();
    (actions, focus)
}

/// Outside fullscreen, J/K/u/d/s/+/-/0/]/[ (`NavConfig::fallback_binds`) scroll the preview and
/// drive diagrams, unless `[nav.binds]` binds that key, under any spelling. Each key stands on
/// its own: binding `u` leaves `d` alone, and `+` and `=` are separate keys.
#[test]
fn test_nav_preview_fallbacks_yield_to_user_binds() {
    use Action::*;
    let keys: &[(&[&str], Action<NullActionExt>)] = &[
        (&["J", "shift-j"], PreviewDown(5)),
        (&["K", "shift-k"], PreviewUp(5)),
        (&["u"], PreviewHalfPageUp),
        (&["d"], PreviewHalfPageDown),
        (&["s"], ToggleDiagram),
        (&["+"], DiagramZoomIn),
        (&["="], DiagramZoomIn),
        (&["-"], DiagramZoomOut),
        (&["0"], DiagramResetZoom),
        (&["]"], NextDiagram),
        (&["n"], NextDiagram),
        (&["["], PrevDiagram),
        (&["N"], PrevDiagram),
    ];
    let user = Action::<NullActionExt>::Pos(42);
    for (spellings, builtin) in keys {
        for pressed in *spellings {
            let (out, _) = nav_key(pressed, &HashMap::new(), false, false);
            assert_eq!(out, vec![builtin.clone()], "{pressed} with no binds");
            for bound in *spellings {
                let binds = HashMap::from([(bound.to_string(), Actions::from([user.clone()]))]);
                let (out, _) = nav_key(pressed, &binds, false, false);
                assert_eq!(out, vec![user.clone()], "{pressed} with {bound} bound");
            }
        }
    }
    let binds = HashMap::from([("u".to_string(), Actions::from([user.clone()]))]);
    assert_eq!(
        nav_key("d", &binds, false, false).0,
        vec![PreviewHalfPageDown]
    );
    let binds = HashMap::from([("+".to_string(), Actions::from([user.clone()]))]);
    assert_eq!(nav_key("=", &binds, false, false).0, vec![DiagramZoomIn]);
}

/// Any key that starts a bound two-key sequence waits for the next key, not only `g`; a key
/// that is also bound on its own fires at once, and the key that breaks a sequence does only
/// what it is bound to.
#[test]
fn test_nav_sequences_with_any_first_key() {
    let binds = HashMap::from([
        ("zz".to_string(), Actions::from([Action::Pos(7)])),
        ("x".to_string(), Actions::from([Action::Pos(1)])),
        ("xy".to_string(), Actions::from([Action::Pos(2)])),
    ]);
    let (keymap, preview) = keymaps(&binds);
    let run = |keys: &[&str]| {
        let mut pending = None;
        let mut focus = Focus::Results;
        let mut out = Vec::new();
        for key in keys {
            crate::render::keys::process_results_nav_key::<NullActionExt>(
                key,
                Some(Action::Char(key.chars().next().unwrap())),
                &keymap,
                &preview,
                &mut pending,
                &mut focus,
                &mut out,
                false,
                false,
            );
        }
        let actions: Vec<_> = out
            .into_iter()
            .filter_map(|c| match c {
                RenderCommand::Action(a) => Some(a),
                _ => None,
            })
            .collect();
        (actions, pending)
    };
    assert_eq!(run(&["z"]), (vec![], Some('z')));
    assert_eq!(run(&["z", "z"]), (vec![Action::Pos(7)], None));
    // `z` then `j`: j does its own bind (Down) and nothing else.
    assert_eq!(run(&["z", "j"]).1, None);
    // `x` is bound on its own, so `xy` never starts.
    assert_eq!(run(&["x", "y"]).0, vec![Action::Pos(1)]);
    // The default g sequences still work.
    assert_eq!(run(&["g", "t"]).0, vec![Action::Pos(0)]);
    assert_eq!(run(&["g", "b"]).0, vec![Action::Pos(-1)]);
}

/// In a fullscreen preview, h/l and H/L scroll sideways by 6 and 18 columns.
#[test]
fn test_nav_fullscreen_horizontal_scroll() {
    let none = HashMap::new();
    for (key, cols) in [("h", -6), ("l", 6), ("H", -18), ("L", 18)] {
        let (out, _) = nav_key(key, &none, true, false);
        assert_eq!(out, vec![Action::PreviewHScroll(cols)], "{key}");
    }
}

/// Esc (any case) leaves fullscreen or diagram preview; Enter leaves only fullscreen, and other
/// keys never do.
#[test]
fn test_nav_preview_exit_keys() {
    let none = HashMap::new();
    for (key, fullscreen, diagram, exits) in [
        ("esc", true, false, true),
        ("Esc", false, true, true),
        ("enter", true, false, true),
        ("enter", false, true, false),
        ("x", true, false, false),
        ("x", false, true, false),
    ] {
        let (out, focus) = nav_key(key, &none, fullscreen, diagram);
        assert_eq!(
            out.contains(&Action::CyclePreview),
            exits,
            "{key} fullscreen={fullscreen} diagram={diagram}: {out:?}"
        );
        assert_eq!(focus == Focus::Input, exits, "{key}: focus {focus:?}");
    }
}

/// Runs a buffer through `apply_focus_binds_mode` and returns the resulting actions.
fn focus_mode(
    cmds: Vec<RenderCommand<NullActionExt>>,
    focus: Focus,
    focus_binds: &HashMap<String, Actions<NullActionExt>>,
    preview_fullscreen: bool,
    preview_diagram_mode: bool,
) -> Vec<Action<NullActionExt>> {
    let mut buffer = cmds;
    let mut pending = None;
    let mut sort_menu_active = false;
    apply_focus_binds_mode(
        &mut buffer,
        focus,
        focus_binds,
        &HashMap::new(),
        false,
        &mut pending,
        &mut sort_menu_active,
        preview_fullscreen,
        preview_diagram_mode,
    );
    buffer
        .into_iter()
        .map(|c| match c {
            RenderCommand::Action(a) | RenderCommand::KeyAction { action: a, .. } => a,
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

/// Editing actions (typed char, backspace, left, right) edit the filter when it has focus, and
/// are read as navigation keys when the results have focus or a preview fills the screen.
#[test]
fn test_nav_editing_actions_follow_focus_and_preview() {
    let binds = HashMap::from([
        ("x".to_string(), Actions::from([Action::Pos(1)])),
        ("backspace".to_string(), Actions::from([Action::Pos(2)])),
        ("left".to_string(), Actions::from([Action::Pos(3)])),
        ("right".to_string(), Actions::from([Action::Pos(4)])),
    ]);
    let cases: [(Action<NullActionExt>, Action<NullActionExt>); 4] = [
        (Action::Char('x'), Action::Pos(1)),
        (Action::DeleteChar, Action::Pos(2)),
        (Action::BackwardChar, Action::Pos(3)),
        (Action::ForwardChar, Action::Pos(4)),
    ];
    for (input, nav) in cases {
        // Filter focused, no preview: the filter gets the edit.
        let out = focus_mode(
            vec![input.clone().into()],
            Focus::Input,
            &binds,
            false,
            false,
        );
        assert_eq!(out, vec![input.clone()], "{input:?} in the filter");
        // Results focused: the nav bind for that key.
        let out = focus_mode(
            vec![input.clone().into()],
            Focus::Results,
            &binds,
            false,
            false,
        );
        assert_eq!(out, vec![nav.clone()], "{input:?} in the results");
    }
    // A fullscreen or diagram preview takes left/right even with the filter focused.
    let none = HashMap::new();
    let out = focus_mode(
        vec![Action::BackwardChar.into()],
        Focus::Input,
        &none,
        true,
        false,
    );
    assert_eq!(out, vec![Action::PreviewHScroll(-6)]);
    let out = focus_mode(
        vec![Action::ForwardChar.into()],
        Focus::Input,
        &none,
        false,
        true,
    );
    assert_eq!(out, vec![Action::PreviewHScroll(6)]);
    // ... and typed keys.
    let out = focus_mode(
        vec![RenderCommand::KeyAction {
            key: "j".into(),
            action: Action::Char('j'),
        }],
        Focus::Input,
        &none,
        false,
        true,
    );
    assert_eq!(out, vec![Action::PreviewDown(3)]);
    let out = focus_mode(
        vec![Action::Char('j').into()],
        Focus::Input,
        &none,
        true,
        false,
    );
    assert_eq!(out, vec![Action::PreviewDown(3)]);
}

/// Up/Down actions scroll a fullscreen or diagram preview; otherwise they move the cursor as
/// sent, even when the user bound "up"/"down" in nav mode (those binds apply to keys).
#[test]
fn test_nav_up_down_actions_scroll_only_a_full_preview() {
    let binds = HashMap::from([
        ("up".to_string(), Actions::from([Action::Pos(8)])),
        ("down".to_string(), Actions::from([Action::Pos(9)])),
    ]);
    for focus in [Focus::Input, Focus::Results] {
        let out = focus_mode(vec![Action::Up(1).into()], focus, &binds, false, false);
        assert_eq!(out, vec![Action::Up(1)], "{focus:?}");
        let out = focus_mode(vec![Action::Down(1).into()], focus, &binds, false, false);
        assert_eq!(out, vec![Action::Down(1)], "{focus:?}");
    }
    let none = HashMap::new();
    let out = focus_mode(
        vec![Action::Up(1).into()],
        Focus::Results,
        &none,
        true,
        false,
    );
    assert_eq!(out, vec![Action::PreviewUp(3)]);
    let out = focus_mode(
        vec![Action::Down(1).into()],
        Focus::Results,
        &none,
        false,
        true,
    );
    assert_eq!(out, vec![Action::PreviewDown(3)]);
}

/// In the results, unbound typed characters and editing keys do nothing, `g b` jumps to the last
/// item, and `g t` to the first.
#[test]
fn test_nav_unbound_keys_in_results() {
    let none = HashMap::new();
    for (key, action) in [
        ("x", Action::Char('x')),
        ("ctrl-w", Action::DeleteWord),
        ("delete", Action::DeleteNextChar),
        ("alt-d", Action::DeleteNextWord),
        ("ctrl-a", Action::DeleteLineStart),
        ("ctrl-e", Action::DeleteLineEnd),
    ] {
        let (out, _) = nav_key_with_fallback(key, action.clone(), &none);
        assert!(
            out.is_empty(),
            "{key} -> {action:?} should be dropped, got {out:?}"
        );
    }
    let gb = focus_mode(
        vec![Action::Char('g').into(), Action::Char('b').into()],
        Focus::Results,
        &none,
        false,
        false,
    );
    assert_eq!(gb, vec![Action::Pos(-1)]);
    let gt = focus_mode(
        vec![Action::Char('g').into(), Action::Char('t').into()],
        Focus::Results,
        &none,
        false,
        false,
    );
    assert_eq!(gt, vec![Action::Pos(0)]);
}

fn nav_key_with_fallback(
    key: &str,
    fallback: Action<NullActionExt>,
    focus_binds: &HashMap<String, Actions<NullActionExt>>,
) -> (Vec<Action<NullActionExt>>, Focus) {
    let mut pending = None;
    let mut focus = Focus::Results;
    let mut out = Vec::new();
    let (keymap, preview) = keymaps(focus_binds);
    crate::render::keys::process_results_nav_key::<NullActionExt>(
        key,
        Some(fallback),
        &keymap,
        &preview,
        &mut pending,
        &mut focus,
        &mut out,
        false,
        false,
    );
    let actions = out
        .into_iter()
        .filter_map(|c| match c {
            RenderCommand::Action(a) => Some(a),
            _ => None,
        })
        .collect();
    (actions, focus)
}

/// Pins every key of a fullscreen or diagram preview, in every spelling the hard-coded table
/// accepts, before those keys become configurable binds (docs/PROPOSAL-UNIFIED-BINDS.md, 3b).
#[test]
fn test_preview_keys_pinned_before_they_become_binds() {
    use Action::*;
    let table: &[(&[&str], Action<NullActionExt>)] = &[
        (&["j", "down", "Down"], PreviewDown(3)),
        (&["k", "up", "Up"], PreviewUp(3)),
        (&["h", "left", "Left"], PreviewHScroll(-6)),
        (&["l", "right", "Right"], PreviewHScroll(6)),
        (
            &["J", "shift-j", "shift-down", "Shift-Down", "Shift-down"],
            PreviewDown(15),
        ),
        (
            &["K", "shift-k", "shift-up", "Shift-Up", "Shift-up"],
            PreviewUp(15),
        ),
        (
            &["H", "shift-left", "Shift-Left", "Shift-left"],
            PreviewHScroll(-18),
        ),
        (
            &["L", "shift-right", "Shift-Right", "Shift-right"],
            PreviewHScroll(18),
        ),
        (&["ctrl-u", "u"], PreviewHalfPageUp),
        (&["ctrl-d", "d"], PreviewHalfPageDown),
        (&["s"], ToggleDiagram),
        (&["n", "]"], NextDiagram),
        (&["N", "["], PrevDiagram),
        (&["+", "="], DiagramZoomIn),
        (&["-"], DiagramZoomOut),
        (&["0"], DiagramResetZoom),
        (&["y"], Accept),
        (&["ctrl-c"], Abort(130)),
        (&["G"], PreviewDown(0)),
    ];
    let none = HashMap::new();
    for (fullscreen, diagram) in [(true, false), (false, true)] {
        for (keys, expected) in table {
            for key in *keys {
                let (out, _) = nav_key(key, &none, fullscreen, diagram);
                assert_eq!(
                    out,
                    vec![expected.clone()],
                    "{key} fullscreen={fullscreen} diagram={diagram}"
                );
            }
        }
        // g g: the first g waits, the second goes to the top.
        let out = focus_mode(
            vec![
                RenderCommand::KeyAction {
                    key: "g".into(),
                    action: Action::Char('g'),
                },
                RenderCommand::KeyAction {
                    key: "g".into(),
                    action: Action::Char('g'),
                },
            ],
            Focus::Results,
            &none,
            fullscreen,
            diagram,
        );
        assert_eq!(out, vec![PreviewUp(0)], "gg fullscreen={fullscreen}");
    }
}

/// A bind that moves focus moves it for the rest of the same input event: a key command after
/// it is resolved for the pane that now has focus.
#[test]
fn test_focus_moves_within_one_event() {
    let binds = HashMap::from([
        ("/".to_string(), Actions::from([Action::FocusFilter])),
        ("j".to_string(), Actions::from([Action::Down(1)])),
    ]);
    let run = |focus: Focus, cmds: Vec<RenderCommand<NullActionExt>>| {
        let mut buffer = cmds;
        let (mut pending, mut sort) = (None, false);
        apply_focus_binds(
            &mut buffer,
            focus,
            &binds,
            false,
            &mut pending,
            &mut sort,
            false,
        );
        buffer
            .into_iter()
            .map(|c| match c {
                RenderCommand::Action(a) => a,
                other => panic!("{other:?}"),
            })
            .collect::<Vec<_>>()
    };
    let key = |k: &str, action: Action<NullActionExt>| RenderCommand::KeyAction {
        key: k.into(),
        action,
    };

    // A nav bind's FocusFilter: the char after it is typed into the filter.
    assert_eq!(
        run(
            Focus::Results,
            vec![
                key("/", Action::Char('/')),
                RenderCommand::Action(Action::Char('j'))
            ]
        ),
        vec![Action::FocusFilter, Action::Char('j')]
    );
    // From the filter, a global ToggleFocus or FocusNav hands the next key to the results.
    for to_results in [Action::ToggleFocus, Action::FocusNav] {
        assert_eq!(
            run(
                Focus::Input,
                vec![key("x", to_results.clone()), key("j", Action::Char('j'))]
            ),
            vec![to_results, Action::Down(1)]
        );
    }
}

/// In diagram mode, characters drive the diagram even while the filter has focus.
#[test]
fn test_diagram_mode_reads_chars_with_the_filter_focused() {
    let out = focus_mode(
        vec![RenderCommand::Action(Action::Char('j'))],
        Focus::Input,
        &HashMap::new(),
        false,
        true,
    );
    assert_eq!(out, vec![Action::PreviewDown(3)]);
}
