//! Turning bound actions into render commands: aliases, sort menu, nav and focus binds.

use super::{Focus, MMState};
use crate::action::{Action, ActionExt, NullActionExt};
use crate::message::RenderCommand;
use crate::{ActionAliaser, SSS, Selection};

pub(super) fn action_from_null<A: ActionExt>(action: Action<NullActionExt>) -> Option<Action<A>> {
    Some(match action {
        Action::Select => Action::Select,
        Action::Deselect => Action::Deselect,
        Action::DeselectUp => Action::DeselectUp,
        Action::Toggle => Action::Toggle,
        Action::ToggleUp => Action::ToggleUp,
        Action::CycleAll => Action::CycleAll,
        Action::ClearSelections => Action::ClearSelections,
        Action::Accept => Action::Accept,
        Action::Quit(x) => Action::Quit(x),
        Action::ToggleWrap => Action::ToggleWrap,
        Action::ToggleActionBox => Action::ToggleActionBox,
        Action::ToggleFocus => Action::ToggleFocus,
        Action::FocusFilter => Action::FocusFilter,
        Action::FocusNav => Action::FocusNav,
        Action::ToggleParentPeek => Action::ToggleParentPeek,
        Action::ToggleFooter => Action::ToggleFooter,
        Action::ToggleHeader => Action::ToggleHeader,
        Action::Up(x) => Action::Up(x),
        Action::Down(x) => Action::Down(x),
        Action::Pos(x) => Action::Pos(x),
        Action::HalfPageDown => Action::HalfPageDown,
        Action::HalfPageUp => Action::HalfPageUp,
        Action::HScroll(x) => Action::HScroll(x),
        Action::VScroll(x) => Action::VScroll(x),
        Action::CyclePreview => Action::CyclePreview,
        Action::Preview(x) => Action::Preview(x),
        Action::Help(x) => Action::Help(x),
        Action::SetPreview(x) => Action::SetPreview(x),
        Action::SwitchPreview(x) => Action::SwitchPreview(x),
        Action::TogglePreviewWrap => Action::TogglePreviewWrap,
        Action::PreviewUp(x) => Action::PreviewUp(x),
        Action::PreviewDown(x) => Action::PreviewDown(x),
        Action::ExpandPreview(x) => Action::ExpandPreview(x),
        Action::ShrinkPreview(x) => Action::ShrinkPreview(x),
        Action::PreviewZoomIn => Action::PreviewZoomIn,
        Action::PreviewZoomOut => Action::PreviewZoomOut,
        Action::PreviewHalfPageUp => Action::PreviewHalfPageUp,
        Action::PreviewHalfPageDown => Action::PreviewHalfPageDown,
        Action::PreviewHScroll(x) => Action::PreviewHScroll(x),
        Action::PreviewScroll(x) => Action::PreviewScroll(x),
        Action::PreviewJump => Action::PreviewJump,
        Action::NextDiagram => Action::NextDiagram,
        Action::PrevDiagram => Action::PrevDiagram,
        Action::DiagramZoomIn => Action::DiagramZoomIn,
        Action::DiagramZoomOut => Action::DiagramZoomOut,
        Action::DiagramResetZoom => Action::DiagramResetZoom,
        Action::PreviewResetZoom => Action::PreviewResetZoom,
        Action::ToggleDiagram => Action::ToggleDiagram,
        Action::NextColumn => Action::NextColumn,
        Action::PrevColumn => Action::PrevColumn,
        Action::SwitchColumn(x) => Action::SwitchColumn(x),
        Action::ToggleColumn(x) => Action::ToggleColumn(x),
        Action::ShowColumn(x) => Action::ShowColumn(x),
        Action::Execute(x) => Action::Execute(x),
        Action::PromptInput(x) => Action::PromptInput(x),
        Action::LiveInput(x) => Action::LiveInput(x),
        Action::LiveInputKey(x) => Action::LiveInputKey(x),
        Action::ExecuteAsync(x) => Action::ExecuteAsync(x),
        Action::ExecuteThen(x) => Action::ExecuteThen(x),
        Action::ExecuteSilent(x) => Action::ExecuteSilent(x),
        Action::Become(x) => Action::Become(x),
        Action::BecomeSilent(x) => Action::BecomeSilent(x),
        Action::Reload(x) => Action::Reload(x),
        Action::ChDir(x) => Action::ChDir(x),
        Action::Print(x) => Action::Print(x),
        Action::PrintKey => Action::PrintKey,
        Action::Store(x) => Action::Store(x),
        Action::ForwardChar => Action::ForwardChar,
        Action::BackwardChar => Action::BackwardChar,
        Action::ForwardWord => Action::ForwardWord,
        Action::BackwardWord => Action::BackwardWord,
        Action::DeleteChar => Action::DeleteChar,
        Action::DeleteWord => Action::DeleteWord,
        Action::DeleteNextChar => Action::DeleteNextChar,
        Action::DeleteNextWord => Action::DeleteNextWord,
        Action::DeleteLineStart => Action::DeleteLineStart,
        Action::DeleteLineEnd => Action::DeleteLineEnd,
        Action::Cancel => Action::Cancel,
        Action::SetQuery(x) => Action::SetQuery(x),
        Action::QueryPos(x) => Action::QueryPos(x),
        Action::Char(x) => Action::Char(x),
        Action::Redraw => Action::Redraw,
        Action::Custom(_) => return None,
        Action::Overlay(x) => Action::Overlay(x),
        Action::Semantic(x) => Action::Semantic(x),
        Action::SetMode(x) => Action::SetMode(x),
        Action::Copy(x) => Action::Copy(x),
        Action::CopyAsync(x) => Action::CopyAsync(x),
        Action::Trace(x) => Action::Trace(x),
        Action::SortMenu => Action::SortMenu,
        Action::Sort(x) => Action::Sort(x),
    })
}

/// Pre-process `buffer` when the sort menu is active: intercept the next key to select a sort order or cancel.
pub(super) fn apply_sort_menu<A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    sort_menu_active: &mut bool,
) {
    if !*sort_menu_active {
        return;
    }

    let mut out = Vec::with_capacity(buffer.len());

    for cmd in buffer.drain(..) {
        match cmd {
            RenderCommand::Tick => {
                out.push(RenderCommand::Tick);
            }
            RenderCommand::Action(Action::Char(c))
            | RenderCommand::KeyAction {
                action: Action::Char(c),
                ..
            } => {
                *sort_menu_active = false;
                match c {
                    'a' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::Alphabetical,
                    )))),
                    'A' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::AlphabeticalReverse,
                    )))),
                    'n' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::Natural,
                    )))),
                    'N' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::NaturalReverse,
                    )))),
                    'm' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::Modified,
                    )))),
                    'M' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::ModifiedReverse,
                    )))),
                    'b' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::Created,
                    )))),
                    'B' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::CreatedReverse,
                    )))),
                    's' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::Size,
                    )))),
                    'S' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::SizeReverse,
                    )))),
                    'e' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::Extension,
                    )))),
                    'E' => out.push(RenderCommand::Action(Action::Sort(Some(
                        crate::action::SortOrder::ExtensionReverse,
                    )))),
                    _ => {
                        // Any other char cancels sort menu
                    }
                }
            }
            RenderCommand::Action(Action::Quit(1))
            | RenderCommand::Action(Action::Cancel)
            | RenderCommand::KeyAction {
                action: Action::Quit(1) | Action::Cancel,
                ..
            } => {
                // Escape or Cancel closes sort menu without exiting the picker
                *sort_menu_active = false;
            }
            RenderCommand::KeyAction { key, .. } if key.eq_ignore_ascii_case("esc") => {
                // Escape closes sort menu without exiting the picker
                *sort_menu_active = false;
            }
            RenderCommand::Action(Action::SortMenu)
            | RenderCommand::KeyAction {
                action: Action::SortMenu,
                ..
            } => {
                *sort_menu_active = false;
            }
            RenderCommand::Action(a) | RenderCommand::KeyAction { action: a, .. } => {
                *sort_menu_active = false;
                out.push(RenderCommand::Action(a));
            }
            other => {
                *sort_menu_active = false;
                out.push(other);
            }
        }
    }

    *buffer = out;
}

#[inline]
pub(super) fn get_nav_bind<'a>(
    focus_binds: &'a std::collections::HashMap<String, crate::action::Actions<NullActionExt>>,
    key: &str,
) -> Option<&'a crate::action::Actions<NullActionExt>> {
    if let Some(actions) = focus_binds.get(key) {
        return Some(actions);
    }
    if key.eq_ignore_ascii_case("space") {
        if let Some(actions) = focus_binds.get(" ") {
            return Some(actions);
        }
    } else if key == " " {
        if let Some(actions) = focus_binds
            .get("space")
            .or_else(|| focus_binds.get("Space"))
        {
            return Some(actions);
        }
    } else if key == "?" {
        if let Some(actions) = focus_binds.get("shift-?") {
            return Some(actions);
        }
    } else if key == "shift-?"
        && let Some(actions) = focus_binds.get("?")
    {
        return Some(actions);
    }
    // For multi-character keys (e.g. "Esc", "Backspace", "Tab", "Ctrl-l", "Left", "Right"),
    // check case-insensitively since crokey capitalizes key names while config/users often use lowercase.
    if key.chars().count() > 1 {
        let lower = key.to_ascii_lowercase();
        if let Some(actions) = focus_binds.get(&lower) {
            return Some(actions);
        }
        return focus_binds
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, actions)| actions);
    }
    None
}

pub(super) fn update_sim_focus<A: ActionExt>(action: &Action<A>, sim_focus: &mut Focus) {
    match action {
        Action::ToggleFocus => {
            *sim_focus = match *sim_focus {
                Focus::Input => Focus::Results,
                Focus::Results => Focus::Input,
            };
        }
        Action::FocusFilter => {
            *sim_focus = Focus::Input;
        }
        Action::FocusNav => {
            *sim_focus = Focus::Results;
        }
        _ => {}
    }
}

pub(super) fn process_results_nav_key<A: ActionExt>(
    key: &str,
    fallback_action: Option<Action<A>>,
    focus_binds: &std::collections::HashMap<String, crate::action::Actions<NullActionExt>>,
    pending_nav_key: &mut Option<char>,
    sort_menu_active: &mut bool,
    sim_focus: &mut Focus,
    out: &mut Vec<RenderCommand<A>>,
    preview_fullscreen: bool,
    preview_diagram_mode: bool,
) {
    if preview_fullscreen || preview_diagram_mode {
        match key {
            "j" | "down" | "Down" => {
                out.push(RenderCommand::Action(Action::PreviewDown(3)));
                return;
            }
            "k" | "up" | "Up" => {
                out.push(RenderCommand::Action(Action::PreviewUp(3)));
                return;
            }
            "h" | "left" | "Left" => {
                out.push(RenderCommand::Action(Action::PreviewHScroll(-6)));
                return;
            }
            "l" | "right" | "Right" => {
                out.push(RenderCommand::Action(Action::PreviewHScroll(6)));
                return;
            }
            "J" | "shift-j" | "shift-down" | "Shift-Down" | "Shift-down" => {
                out.push(RenderCommand::Action(Action::PreviewDown(15)));
                return;
            }
            "K" | "shift-k" | "shift-up" | "Shift-Up" | "Shift-up" => {
                out.push(RenderCommand::Action(Action::PreviewUp(15)));
                return;
            }
            "H" | "shift-left" | "Shift-Left" | "Shift-left" => {
                out.push(RenderCommand::Action(Action::PreviewHScroll(-18)));
                return;
            }
            "L" | "shift-right" | "Shift-Right" | "Shift-right" => {
                out.push(RenderCommand::Action(Action::PreviewHScroll(18)));
                return;
            }
            "ctrl-u" | "u" => {
                out.push(RenderCommand::Action(Action::PreviewHalfPageUp));
                return;
            }
            "ctrl-d" | "d" => {
                out.push(RenderCommand::Action(Action::PreviewHalfPageDown));
                return;
            }
            "g" => {
                if *pending_nav_key == Some('g') {
                    *pending_nav_key = None;
                    out.push(RenderCommand::Action(Action::PreviewUp(0)));
                } else {
                    *pending_nav_key = Some('g');
                }
                return;
            }
            "G" => {
                *pending_nav_key = None;
                out.push(RenderCommand::Action(Action::PreviewDown(0)));
                return;
            }
            "s" => {
                out.push(RenderCommand::Action(Action::ToggleDiagram));
                return;
            }
            "n" | "]" => {
                out.push(RenderCommand::Action(Action::NextDiagram));
                return;
            }
            "N" | "[" => {
                out.push(RenderCommand::Action(Action::PrevDiagram));
                return;
            }
            "+" | "=" => {
                out.push(RenderCommand::Action(Action::DiagramZoomIn));
                return;
            }
            "-" => {
                out.push(RenderCommand::Action(Action::DiagramZoomOut));
                return;
            }
            "0" => {
                out.push(RenderCommand::Action(Action::DiagramResetZoom));
                return;
            }
            "y" => {
                out.push(RenderCommand::Action(Action::Accept));
                return;
            }
            "ctrl-c" => {
                out.push(RenderCommand::Action(Action::Quit(130)));
                return;
            }
            k if k.eq_ignore_ascii_case("esc") || (preview_fullscreen && k == "enter") => {
                *sim_focus = Focus::Input;
                out.push(RenderCommand::Action(Action::CyclePreview));
                return;
            }
            k => {
                *pending_nav_key = None;
                if let Some(actions) = get_nav_bind(focus_binds, k) {
                    for action in actions.iter().cloned() {
                        if let Some(action) = action_from_null::<A>(action) {
                            update_sim_focus(&action, sim_focus);
                            out.push(RenderCommand::Action(action));
                        }
                    }
                    return;
                }
                if let Some(action) = fallback_action {
                    update_sim_focus(&action, sim_focus);
                    out.push(RenderCommand::Action(action));
                }
                return;
            }
        }
    } else {
        // Non-fullscreen preview scrolling and diagram controls fallback
        if (key == "J" || key == "shift-j" || key == "ctrl-shift-j" || key == "ctrl-J")
            && !focus_binds.contains_key("J")
            && !focus_binds.contains_key("shift-j")
            && !focus_binds.contains_key("ctrl-shift-j")
            && !focus_binds.contains_key("ctrl-J")
        {
            out.push(RenderCommand::Action(Action::PreviewDown(5)));
            return;
        }
        if (key == "K" || key == "shift-k" || key == "ctrl-shift-k" || key == "ctrl-K")
            && !focus_binds.contains_key("K")
            && !focus_binds.contains_key("shift-k")
            && !focus_binds.contains_key("ctrl-shift-k")
            && !focus_binds.contains_key("ctrl-K")
        {
            out.push(RenderCommand::Action(Action::PreviewUp(5)));
            return;
        }
        if (key == "ctrl-u" || key == "u")
            && !focus_binds.contains_key("ctrl-u")
            && !focus_binds.contains_key("u")
        {
            out.push(RenderCommand::Action(Action::PreviewHalfPageUp));
            return;
        }
        if (key == "ctrl-d" || key == "d")
            && !focus_binds.contains_key("ctrl-d")
            && !focus_binds.contains_key("d")
        {
            out.push(RenderCommand::Action(Action::PreviewHalfPageDown));
            return;
        }
        if key == "s" && !focus_binds.contains_key("s") {
            out.push(RenderCommand::Action(Action::ToggleDiagram));
            return;
        }
        if (key == "+" || key == "=")
            && !focus_binds.contains_key("+")
            && !focus_binds.contains_key("=")
        {
            out.push(RenderCommand::Action(Action::DiagramZoomIn));
            return;
        }
        if key == "-" && !focus_binds.contains_key("-") {
            out.push(RenderCommand::Action(Action::DiagramZoomOut));
            return;
        }
        if key == "0" && !focus_binds.contains_key("0") {
            out.push(RenderCommand::Action(Action::DiagramResetZoom));
            return;
        }
        if (key == "]" || key == "n") && !focus_binds.contains_key(key) {
            out.push(RenderCommand::Action(Action::NextDiagram));
            return;
        }
        if (key == "[" || key == "N") && !focus_binds.contains_key(key) {
            out.push(RenderCommand::Action(Action::PrevDiagram));
            return;
        }
    }

    if key == "," {
        *pending_nav_key = None;
        *sort_menu_active = true;
        out.push(RenderCommand::Action(Action::SortMenu));
        return;
    }
    match pending_nav_key.take() {
        Some('g') => {
            if key == "g" {
                let seq = "gg".to_string();
                if let Some(actions) = get_nav_bind(focus_binds, &seq) {
                    for action in actions.iter().cloned() {
                        if let Some(action) = action_from_null::<A>(action) {
                            update_sim_focus(&action, sim_focus);
                            out.push(RenderCommand::Action(action));
                        }
                    }
                } else {
                    out.push(RenderCommand::Action(Action::PreviewUp(0)));
                }
            } else if key == "t" {
                let seq = "gt".to_string();
                if let Some(actions) = get_nav_bind(focus_binds, &seq) {
                    for action in actions.iter().cloned() {
                        if let Some(action) = action_from_null::<A>(action) {
                            update_sim_focus(&action, sim_focus);
                            out.push(RenderCommand::Action(action));
                        }
                    }
                } else {
                    out.push(RenderCommand::Action(Action::Pos(0)));
                }
            } else if key == "b" {
                let seq = "gb".to_string();
                if let Some(actions) = get_nav_bind(focus_binds, &seq) {
                    for action in actions.iter().cloned() {
                        if let Some(action) = action_from_null::<A>(action) {
                            update_sim_focus(&action, sim_focus);
                            out.push(RenderCommand::Action(action));
                        }
                    }
                } else {
                    out.push(RenderCommand::Action(Action::Pos(-1)));
                }
            } else if let Some(actions) = get_nav_bind(focus_binds, key) {
                for action in actions.iter().cloned() {
                    if let Some(action) = action_from_null::<A>(action) {
                        update_sim_focus(&action, sim_focus);
                        out.push(RenderCommand::Action(action));
                    }
                }
            }
        }
        _ => {
            if key == "g" && !focus_binds.contains_key("g") {
                *pending_nav_key = Some('g');
            } else if let Some(actions) = get_nav_bind(focus_binds, key) {
                for action in actions.iter().cloned() {
                    if let Some(action) = action_from_null::<A>(action) {
                        update_sim_focus(&action, sim_focus);
                        out.push(RenderCommand::Action(action));
                    }
                }
            } else if let Some(action) = fallback_action {
                match &action {
                    Action::ToggleFocus => {
                        if key.eq_ignore_ascii_case("esc") {
                            out.push(RenderCommand::Action(Action::Quit(130)));
                        } else {
                            *sim_focus = Focus::Input;
                            out.push(RenderCommand::Action(Action::ToggleFocus));
                        }
                    }
                    Action::FocusFilter => {
                        *sim_focus = Focus::Input;
                        out.push(RenderCommand::Action(Action::FocusFilter));
                    }
                    Action::FocusNav => {
                        *sim_focus = Focus::Results;
                        out.push(RenderCommand::Action(Action::FocusNav));
                    }
                    Action::Char(_) => {
                        // Drop unbound plain characters in nav mode
                    }
                    Action::DeleteChar
                    | Action::DeleteWord
                    | Action::DeleteNextChar
                    | Action::DeleteNextWord
                    | Action::DeleteLineStart
                    | Action::DeleteLineEnd => {
                        // Drop unbound editing actions in nav mode
                    }
                    _ => {
                        update_sim_focus(&action, sim_focus);
                        out.push(RenderCommand::Action(action));
                    }
                }
            }
        }
    }
}

/// Pre-process `buffer` for navigation mode: simulate `ToggleFocus` events encountered in the
/// batch and expand `Action::Char` events into `nav_binds` actions while focus is on results.
#[inline]
#[allow(dead_code)]
pub(super) fn apply_focus_binds<A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    initial_focus: Focus,
    focus_binds: &std::collections::HashMap<String, crate::action::Actions<NullActionExt>>,
    overlay_active: bool,
    pending_nav_key: &mut Option<char>,
    sort_menu_active: &mut bool,
    preview_fullscreen: bool,
) {
    apply_focus_binds_mode(
        buffer,
        initial_focus,
        focus_binds,
        overlay_active,
        pending_nav_key,
        sort_menu_active,
        preview_fullscreen,
        false,
    );
}

pub(super) fn apply_focus_binds_mode<A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    initial_focus: Focus,
    focus_binds: &std::collections::HashMap<String, crate::action::Actions<NullActionExt>>,
    overlay_active: bool,
    pending_nav_key: &mut Option<char>,
    sort_menu_active: &mut bool,
    preview_fullscreen: bool,
    preview_diagram_mode: bool,
) {
    if overlay_active {
        for cmd in buffer {
            if let RenderCommand::KeyAction { action, .. } = cmd {
                *cmd = RenderCommand::Action(action.clone());
            }
        }
        return;
    }

    if *sort_menu_active {
        apply_sort_menu(buffer, sort_menu_active);
        return;
    }

    let mut out = Vec::with_capacity(buffer.len());
    let mut sim_focus = if preview_fullscreen {
        Focus::Results
    } else {
        initial_focus
    };
    let mut last_consumed_nav_key: Option<String> = None;

    for cmd in buffer.drain(..) {
        match cmd {
            RenderCommand::Tick => {
                last_consumed_nav_key = None;
                out.push(RenderCommand::Tick);
            }
            RenderCommand::Action(Action::Trace(t)) => {
                // Traces do not reset last_consumed_nav_key as they can wrap actions
                out.push(RenderCommand::Action(Action::Trace(t)));
            }
            RenderCommand::Action(Action::ToggleFocus) => {
                last_consumed_nav_key = None;
                if preview_fullscreen {
                    sim_focus = Focus::Input;
                    out.push(RenderCommand::Action(Action::CyclePreview));
                    continue;
                }
                if sim_focus == Focus::Results {
                    if let Some(actions) = get_nav_bind(focus_binds, "esc") {
                        for action in actions.iter().cloned() {
                            if let Some(action) = action_from_null::<A>(action) {
                                update_sim_focus(&action, &mut sim_focus);
                                out.push(RenderCommand::Action(action));
                            }
                        }
                    } else {
                        out.push(RenderCommand::Action(Action::Quit(130)));
                    }
                } else {
                    sim_focus = match sim_focus {
                        Focus::Input => Focus::Results,
                        Focus::Results => Focus::Input,
                    };
                    out.push(RenderCommand::Action(Action::ToggleFocus));
                }
            }
            RenderCommand::Action(Action::FocusFilter) => {
                last_consumed_nav_key = None;
                sim_focus = Focus::Input;
                out.push(RenderCommand::Action(Action::FocusFilter));
            }
            RenderCommand::Action(Action::FocusNav) => {
                last_consumed_nav_key = None;
                sim_focus = Focus::Results;
                out.push(RenderCommand::Action(Action::FocusNav));
            }
            RenderCommand::Action(Action::ChDir(payload)) => {
                last_consumed_nav_key = None;
                sim_focus = Focus::Results;
                out.push(RenderCommand::Action(Action::ChDir(payload)));
            }
            RenderCommand::Action(Action::Char(c))
                if sim_focus == Focus::Results || preview_fullscreen || preview_diagram_mode =>
            {
                last_consumed_nav_key = None;
                let key = c.to_string();
                process_results_nav_key(
                    &key,
                    Some(Action::Char(c)),
                    focus_binds,
                    pending_nav_key,
                    sort_menu_active,
                    &mut sim_focus,
                    &mut out,
                    preview_fullscreen,
                    preview_diagram_mode,
                );
            }
            RenderCommand::KeyAction { key, action }
                if sim_focus == Focus::Results || preview_fullscreen || preview_diagram_mode =>
            {
                if !preview_fullscreen
                    && last_consumed_nav_key
                        .as_deref()
                        .is_some_and(|k| k.eq_ignore_ascii_case(&key))
                {
                    continue;
                }
                process_results_nav_key(
                    &key,
                    Some(action),
                    focus_binds,
                    pending_nav_key,
                    sort_menu_active,
                    &mut sim_focus,
                    &mut out,
                    preview_fullscreen,
                    preview_diagram_mode,
                );
                if !preview_fullscreen && get_nav_bind(focus_binds, &key).is_some() {
                    last_consumed_nav_key = Some(key);
                } else {
                    last_consumed_nav_key = None;
                }
            }
            RenderCommand::KeyAction { key: _, action } => {
                // sim_focus == Focus::Input
                last_consumed_nav_key = None;
                update_sim_focus(&action, &mut sim_focus);
                out.push(RenderCommand::Action(action));
            }
            RenderCommand::Action(Action::DeleteChar)
                if sim_focus == Focus::Results || preview_fullscreen || preview_diagram_mode =>
            {
                last_consumed_nav_key = None;
                process_results_nav_key(
                    "backspace",
                    Some(Action::DeleteChar),
                    focus_binds,
                    pending_nav_key,
                    sort_menu_active,
                    &mut sim_focus,
                    &mut out,
                    preview_fullscreen,
                    preview_diagram_mode,
                );
            }
            RenderCommand::Action(Action::BackwardChar)
                if sim_focus == Focus::Results || preview_fullscreen || preview_diagram_mode =>
            {
                last_consumed_nav_key = None;
                process_results_nav_key(
                    "left",
                    Some(Action::BackwardChar),
                    focus_binds,
                    pending_nav_key,
                    sort_menu_active,
                    &mut sim_focus,
                    &mut out,
                    preview_fullscreen,
                    preview_diagram_mode,
                );
            }
            RenderCommand::Action(Action::ForwardChar)
                if sim_focus == Focus::Results || preview_fullscreen || preview_diagram_mode =>
            {
                last_consumed_nav_key = None;
                process_results_nav_key(
                    "right",
                    Some(Action::ForwardChar),
                    focus_binds,
                    pending_nav_key,
                    sort_menu_active,
                    &mut sim_focus,
                    &mut out,
                    preview_fullscreen,
                    preview_diagram_mode,
                );
            }
            RenderCommand::Action(Action::Up(n)) if preview_fullscreen || preview_diagram_mode => {
                last_consumed_nav_key = None;
                process_results_nav_key(
                    "up",
                    Some(Action::Up(n)),
                    focus_binds,
                    pending_nav_key,
                    sort_menu_active,
                    &mut sim_focus,
                    &mut out,
                    preview_fullscreen,
                    preview_diagram_mode,
                );
            }
            RenderCommand::Action(Action::Down(n))
                if preview_fullscreen || preview_diagram_mode =>
            {
                last_consumed_nav_key = None;
                process_results_nav_key(
                    "down",
                    Some(Action::Down(n)),
                    focus_binds,
                    pending_nav_key,
                    sort_menu_active,
                    &mut sim_focus,
                    &mut out,
                    preview_fullscreen,
                    preview_diagram_mode,
                );
            }
            other => {
                last_consumed_nav_key = None;
                out.push(other);
            }
        }
    }

    *buffer = out;
}

pub(super) fn apply_aliases<T: SSS, S: Selection, A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    aliaser: &mut ActionAliaser<T, S, A>,
    dispatcher: &mut MMState<'_, '_, T, S>,
) {
    let mut out = Vec::new();

    for cmd in buffer.drain(..) {
        match cmd {
            RenderCommand::Action(a) => out.extend(
                aliaser(a, dispatcher)
                    .into_iter()
                    .map(RenderCommand::Action),
            ),
            RenderCommand::KeyAction { key, action } => {
                let transformed = aliaser(action, dispatcher);
                for a in transformed {
                    out.push(RenderCommand::KeyAction {
                        key: key.clone(),
                        action: a,
                    });
                }
            }
            other => out.push(other),
        }
    }

    *buffer = out;
}
