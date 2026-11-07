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
        Action::Abort(x) => Action::Abort(x),
        Action::Unwind(x) => Action::Unwind(x),
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
pub(crate) fn apply_sort_menu<A: ActionExt>(
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

/// Emits the actions of a nav bind, tracking the focus each one moves to.
fn push_nav_actions<A: ActionExt>(
    actions: &crate::action::Actions<NullActionExt>,
    sim_focus: &mut Focus,
    out: &mut Vec<RenderCommand<A>>,
) {
    for action in actions.iter().cloned() {
        if let Some(action) = action_from_null::<A>(action) {
            update_sim_focus(&action, sim_focus);
            out.push(RenderCommand::Action(action));
        }
    }
}

#[inline]
#[cfg(test)]
pub(crate) fn get_nav_bind<'a>(
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

/// Resolves one key while the results have focus or a preview fills the screen. `fallback_action`
/// is what the global binds made of the key; it applies when no nav or preview bind takes it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_results_nav_key<A: ActionExt>(
    key: &str,
    fallback_action: Option<Action<A>>,
    focus_binds: &super::nav_keymap::NavKeymap,
    preview_binds: &super::nav_keymap::NavKeymap,
    pending_nav_key: &mut Option<char>,
    sim_focus: &mut Focus,
    out: &mut Vec<RenderCommand<A>>,
    preview_fullscreen: bool,
    preview_diagram_mode: bool,
) {
    use super::nav_keymap::Step;

    if preview_fullscreen || preview_diagram_mode {
        match preview_binds.step(pending_nav_key, key) {
            Step::Sequence(actions) => return push_nav_actions(actions, sim_focus, out),
            Step::Waiting => return,
            Step::Broken | Step::Key => {}
        }
        if let Some(actions) = preview_binds.get(key) {
            return push_nav_actions(actions, sim_focus, out);
        }
        if key.eq_ignore_ascii_case("esc") || (preview_fullscreen && key == "enter") {
            *sim_focus = Focus::Input;
            out.push(RenderCommand::Action(Action::CyclePreview));
            return;
        }
        if let Some(actions) = focus_binds.get(key) {
            return push_nav_actions(actions, sim_focus, out);
        }
        if let Some(action) = fallback_action {
            update_sim_focus(&action, sim_focus);
            out.push(RenderCommand::Action(action));
        }
        return;
    }

    match focus_binds.step(pending_nav_key, key) {
        Step::Sequence(actions) => return push_nav_actions(actions, sim_focus, out),
        Step::Waiting => return,
        // The key after a waiting key does only what it is bound to.
        Step::Broken => {
            if let Some(actions) = focus_binds.get(key) {
                push_nav_actions(actions, sim_focus, out);
            }
            return;
        }
        Step::Key => {}
    }
    if let Some(actions) = focus_binds.get(key) {
        return push_nav_actions(actions, sim_focus, out);
    }
    let Some(action) = fallback_action else {
        return;
    };
    match &action {
        Action::ToggleFocus => {
            if key.eq_ignore_ascii_case("esc") {
                out.push(RenderCommand::Action(Action::Quit(130)));
            } else {
                *sim_focus = Focus::Input;
                out.push(RenderCommand::Action(Action::ToggleFocus));
            }
        }
        // Unbound plain characters and editing actions do nothing in the results.
        Action::Char(_)
        | Action::DeleteChar
        | Action::DeleteWord
        | Action::DeleteNextChar
        | Action::DeleteNextWord
        | Action::DeleteLineStart
        | Action::DeleteLineEnd => {}
        _ => {
            update_sim_focus(&action, sim_focus);
            out.push(RenderCommand::Action(action));
        }
    }
}

/// Resolves navigation-mode keys one command at a time; the event loop runs one per input
/// event (`EventLoop::unit`). It carries the state that spans the event's commands: the
/// simulated focus (a bind's focus action moves it before the render loop applies it) and
/// whether a nav bind took the event's key (the global bind's further actions on that key are
/// then dropped; every key command of one event carries the same key).
pub(crate) struct NavResolver<'b> {
    focus_binds: &'b super::nav_keymap::NavKeymap,
    preview_binds: &'b super::nav_keymap::NavKeymap,
    preview_fullscreen: bool,
    preview_diagram_mode: bool,
    sim_focus: Focus,
    key_taken: bool,
}

impl<'b> NavResolver<'b> {
    pub(crate) fn new(
        initial_focus: Focus,
        focus_binds: &'b super::nav_keymap::NavKeymap,
        preview_binds: &'b super::nav_keymap::NavKeymap,
        preview_fullscreen: bool,
        preview_diagram_mode: bool,
    ) -> Self {
        Self {
            focus_binds,
            preview_binds,
            preview_fullscreen,
            preview_diagram_mode,
            sim_focus: if preview_fullscreen {
                Focus::Results
            } else {
                initial_focus
            },
            key_taken: false,
        }
    }

    /// Resolves one command into `out`.
    pub(crate) fn feed<A: ActionExt>(
        &mut self,
        cmd: RenderCommand<A>,
        pending_nav_key: &mut Option<char>,
        out: &mut Vec<RenderCommand<A>>,
    ) {
        let focus_binds = self.focus_binds;
        let preview_binds = self.preview_binds;
        let (preview_fullscreen, preview_diagram_mode) =
            (self.preview_fullscreen, self.preview_diagram_mode);
        let mut sim_focus = self.sim_focus;
        let mut key_taken = self.key_taken;
        'feed: {
            match cmd {
                RenderCommand::Tick => {
                    key_taken = false;
                    out.push(RenderCommand::Tick);
                }
                RenderCommand::Action(Action::Trace(t)) => {
                    // Traces do not reset key_taken as they can wrap actions
                    out.push(RenderCommand::Action(Action::Trace(t)));
                }
                RenderCommand::Action(Action::ToggleFocus) => {
                    key_taken = false;
                    if preview_fullscreen {
                        sim_focus = Focus::Input;
                        out.push(RenderCommand::Action(Action::CyclePreview));
                        break 'feed;
                    }
                    if sim_focus == Focus::Results {
                        if let Some(actions) = focus_binds.get("esc") {
                            push_nav_actions(actions, &mut sim_focus, out);
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
                    key_taken = false;
                    sim_focus = Focus::Input;
                    out.push(RenderCommand::Action(Action::FocusFilter));
                }
                RenderCommand::Action(Action::FocusNav) => {
                    key_taken = false;
                    sim_focus = Focus::Results;
                    out.push(RenderCommand::Action(Action::FocusNav));
                }
                RenderCommand::Action(Action::ChDir(payload)) => {
                    key_taken = false;
                    sim_focus = Focus::Results;
                    out.push(RenderCommand::Action(Action::ChDir(payload)));
                }
                RenderCommand::Action(Action::Char(c))
                    if sim_focus == Focus::Results
                        || preview_fullscreen
                        || preview_diagram_mode =>
                {
                    key_taken = false;
                    let key = c.to_string();
                    process_results_nav_key(
                        &key,
                        Some(Action::Char(c)),
                        focus_binds,
                        preview_binds,
                        pending_nav_key,
                        &mut sim_focus,
                        out,
                        preview_fullscreen,
                        preview_diagram_mode,
                    );
                }
                RenderCommand::KeyAction { key, action }
                    if sim_focus == Focus::Results
                        || preview_fullscreen
                        || preview_diagram_mode =>
                {
                    if !preview_fullscreen && key_taken {
                        break 'feed;
                    }
                    process_results_nav_key(
                        &key,
                        Some(action),
                        focus_binds,
                        preview_binds,
                        pending_nav_key,
                        &mut sim_focus,
                        out,
                        preview_fullscreen,
                        preview_diagram_mode,
                    );
                    key_taken = !preview_fullscreen && focus_binds.get(&key).is_some();
                }
                RenderCommand::KeyAction { key: _, action } => {
                    // sim_focus == Focus::Input
                    key_taken = false;
                    update_sim_focus(&action, &mut sim_focus);
                    out.push(RenderCommand::Action(action));
                }
                // Editing actions read as navigation keys in the results or a full preview.
                RenderCommand::Action(
                    action @ (Action::DeleteChar | Action::BackwardChar | Action::ForwardChar),
                ) if sim_focus == Focus::Results || preview_fullscreen || preview_diagram_mode => {
                    key_taken = false;
                    let key = match action {
                        Action::DeleteChar => "backspace",
                        Action::BackwardChar => "left",
                        _ => "right",
                    };
                    process_results_nav_key(
                        key,
                        Some(action),
                        focus_binds,
                        preview_binds,
                        pending_nav_key,
                        &mut sim_focus,
                        out,
                        preview_fullscreen,
                        preview_diagram_mode,
                    );
                }
                // Up/Down scroll a fullscreen or diagram preview.
                RenderCommand::Action(action @ (Action::Up(_) | Action::Down(_)))
                    if preview_fullscreen || preview_diagram_mode =>
                {
                    key_taken = false;
                    let key = if matches!(action, Action::Up(_)) {
                        "up"
                    } else {
                        "down"
                    };
                    process_results_nav_key(
                        key,
                        Some(action),
                        focus_binds,
                        preview_binds,
                        pending_nav_key,
                        &mut sim_focus,
                        out,
                        preview_fullscreen,
                        preview_diagram_mode,
                    );
                }
                other => {
                    key_taken = false;
                    out.push(other);
                }
            }
        }
        self.sim_focus = sim_focus;
        self.key_taken = key_taken;
    }
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
