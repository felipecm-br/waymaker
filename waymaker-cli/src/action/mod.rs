use std::{path::PathBuf, process::Command, str::FromStr};
use waymaker::render::{AfterExecute, CommandKind};

use cba::{
    StringError, bait::ResultExt, bring::split::split_on_unescaped_delimiter, broc::CommandExt,
    unwrap,
};
use log::{debug, error};
use waymaker::{
    Action, Actions, ConfigMMInnerItem, ConfigMMItem,
    binds::Trigger,
    config::PartialRenderConfig,
    event::BindSender,
    message::{BindDirective, Interrupt, RenderCommand},
    nucleo::Line,
    ui::StatusUI,
};
use waymaker_partial::{Apply, Set};

use waymaker::preview::AppendOnly;

mod fm;
mod reload;
use fm::*;

pub type MMState<'a, 'b> = waymaker::render::MMState<'a, 'b, ConfigMMItem, ConfigMMInnerItem>;

#[derive(Debug, Clone, PartialEq)]
pub enum MMAction {
    // binds
    /// define a bind
    Bind(String),
    /// unset a bind
    Unbind(String),
    /// append actions to a bind
    PushBind(String),
    /// pop an action from a bind
    PopBind(String),

    // state
    /// Toggle refiltering of results by query.
    Filtering(Option<bool>),
    /// Cycle result sorting between None, Partial, and Full
    CycleSort,
    ReloadNext(Option<usize>),
    ReloadPrev,

    // set
    /// Set header
    SetHeader(Option<String>),
    /// Push header
    PushHeader(String),
    /// Set footer
    SetFooter(Option<String>),
    /// Push footer
    PushFooter(String),
    /// Set footer from a style template, like the config's `[footer] content`
    SetStyledFooter(String),
    /// Set status without interpreting style braces
    SetPrompt(Option<String>),
    /// Set prompt
    SetStyledPrompt(String),
    /// Set status without interpreting style braces
    SetStatus(Option<String>),
    /// Set status
    SetStyledStatus(String),
    /// Set current mode index (0 = local, 1 = frecency, 2 = bookmarks)
    SetModeIndex(usize),
    /// Run a command and display output in preview window
    RunPreview(String),

    /// Accept current selection and print using output_template
    Accept,

    /// [`waymaker::Action::Execute`], confirm on error
    ExecuteOrConfirm(String),
    /// [`waymaker::Action::Execute`], quit on success
    ExecuteAndQuit(String),
    /// [`waymaker::Action::Execute`], quit on success, confirm on error
    BecomeOr(String),
    /// Alias for BecomeOr: execute, quit on success, confirm on error
    BecomeOrConfirm(String),
    /// Alias for BecomeOr: execute, quit on success, resume on interrupt
    BecomeOrResume(String),
    /// Execute command and parse output as actions
    Transform(String),
    /// Execute command and parse output as configuration
    TransformConfig(String),

    /// Set the set of col-0 paths shown with the yank prefix style (FM mode).
    /// Value is a newline-separated list of paths (empty string clears).
    FmSetYankPaths(String),
    FmRemoveYankPaths(String),
    FmSetCutPaths(String),
    FmRemoveCutPaths(String),
    FmSetPinPaths(String),

    /// File-manager action-box operations.
    FmCreateStart,
    FmDeleteStart,
    FmRenameStart,
    FmUnzipStart,
    FmZipStart,
    FmYank,
    FmUnyank,
    FmCut,
    FmUncut,
    FmPaste,
    FmPasteInto,
    FmSetFlashCopy(String),
    FmSetFlashCut(String),
    FmUndo,
    FmRedo,
    FmDragDrop,
    FmTogglePin,
    ReloadReady(Vec<String>),
    Confirm(String),
    Prompt(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FmActionMode {
    Create,
    Delete {
        paths: Vec<String>,
    },
    Rename {
        from: String,
        remaining: Vec<String>,
    },
    Unzip {
        src: String,
    },
    Zip {
        paths: Vec<String>,
    },
    Custom {
        command: String,
    },
}

#[derive(Debug, Clone)]
pub struct ModeHistory {
    pub query: String,
    pub query_cursor: usize,
    pub results_index: u32,
    pub focused_item: Option<String>,
    pub focus: waymaker::render::Focus,
}

pub struct ActionContext {
    pub bind_tx: BindSender<MMAction>,
    pub render_tx: waymaker::event::RenderSender<MMAction>,
    pub additional_commands: (Vec<String>, usize),
    pub output_template: Option<String>,
    pub print_handle: AppendOnly<String>,
    pub output_separator: String,
    pub clipboard: crate::fm::Clipboard,
    pub fm_notify: bool,
    pub undo_stack: crate::fm::UndoStack,
    pub redo_stack: crate::fm::UndoStack,
    pub fm_action: Option<FmActionMode>,
    pub mode_history: std::collections::HashMap<usize, ModeHistory>,
    pub last_cwd: Option<std::path::PathBuf>,
    /// Cursor memory across directory changes and reloads, shared with the start handlers.
    pub nav: crate::start::SharedNav,
}

pub fn action_handler(a: MMAction, state: &mut MMState<'_, '_>, ctx: &mut ActionContext) {
    let ActionContext {
        bind_tx,
        mode_history,
        last_cwd,
        ..
    } = ctx;
    if let Ok(cwd) = std::env::current_dir()
        && last_cwd.as_ref() != Some(&cwd)
    {
        *last_cwd = Some(cwd);
        mode_history.clear();
    }
    match a {
        a @ (MMAction::FmSetYankPaths(..)
        | MMAction::FmRemoveYankPaths(..)
        | MMAction::FmRemoveCutPaths(..)
        | MMAction::FmSetCutPaths(..)
        | MMAction::FmSetPinPaths(..)
        | MMAction::FmSetFlashCopy(..)
        | MMAction::FmSetFlashCut(..)
        | MMAction::FmCreateStart
        | MMAction::FmDeleteStart
        | MMAction::FmRenameStart
        | MMAction::FmUnzipStart
        | MMAction::FmZipStart
        | MMAction::FmYank
        | MMAction::FmUnyank
        | MMAction::FmUncut
        | MMAction::FmCut
        | MMAction::FmPaste
        | MMAction::FmPasteInto
        | MMAction::FmUndo
        | MMAction::FmRedo
        | MMAction::FmDragDrop
        | MMAction::FmTogglePin) => fm::handle_fm(a, state, ctx),
        a @ (MMAction::ReloadNext(..) | MMAction::ReloadPrev | MMAction::ReloadReady(..)) => {
            reload::handle_reload(a, state, ctx)
        }
        MMAction::Accept => accept(state, ctx),
        // state
        MMAction::CycleSort => {
            let threshold = match state.picker_ui.worker.get_stability() {
                0 => 6,
                u32::MAX => 0,
                _ => u32::MAX,
            };
            state
                .picker_ui
                .worker
                .set_stability(waymaker::config::SortThreshold(threshold));
        }
        MMAction::Filtering(s) => {
            if let Some(s) = s {
                state.filtering = s;
            } else {
                state.filtering = !state.filtering;
            }
            if !state.filtering {
                state.picker_ui.worker.find("");
            }
        }

        MMAction::RunPreview(cmd) => {
            if let Some(p) = state.preview_ui {
                p.show(true);
                state.update_preview_set(Ok(cmd));
            }
        }

        // binds
        MMAction::Bind(s) => {
            let (trigger, values) = unwrap!(parse_bind_parts(&s)._elog());
            let _ = bind_tx.send(BindDirective::Bind(trigger, values));
        }
        MMAction::Unbind(s) => {
            let trigger = unwrap!(s.parse()._elog());
            let _ = bind_tx.send(BindDirective::Unbind(trigger));
        }
        MMAction::PushBind(s) => {
            let (trigger, action) = unwrap!(parse_push_bind_parts(&s)._elog());
            let _ = bind_tx.send(BindDirective::PushBind(trigger, action));
        }
        MMAction::PopBind(s) => {
            let trigger = unwrap!(s.parse()._elog());
            let _ = bind_tx.send(BindDirective::PopBind(trigger));
        }

        // set
        MMAction::SetHeader(context) => {
            if let Some(s) = context {
                state.picker_ui.header.set(s);
            } else {
                state.picker_ui.header.clear(true);
            }
        }
        MMAction::PushHeader(s) => {
            state.picker_ui.header.push(s);
        }
        MMAction::SetFooter(context) => {
            if let Some(s) = context {
                state.footer_ui.set(s);
            } else {
                state.footer_ui.clear(false);
            }
        }
        MMAction::PushFooter(s) => {
            state.footer_ui.push(s);
        }
        MMAction::SetStyledFooter(s) => {
            state.footer_ui.set_styled(&s);
        }
        MMAction::SetStyledPrompt(s) => {
            state
                .picker_ui
                .query
                .set_prompt(Some(StatusUI::parse_template_to_status_line(&s)));
        }
        MMAction::SetStyledStatus(s) => {
            state
                .picker_ui
                .results
                .set_status_line(Some(StatusUI::parse_template_to_status_line(&s)));
        }
        MMAction::SetModeIndex(index) => {
            state.picker_ui.query.set_mode_index(index);
            state.picker_ui.results.set_mode_index(index);
            state.picker_ui.worker.set_mode_index(index);
            state.picker_ui.results.set_status_line(None);
        }
        MMAction::SetStatus(s) => {
            state.picker_ui.results.set_status_line(s.map(Line::raw));
        }
        MMAction::SetPrompt(s) => {
            state.picker_ui.query.set_prompt(s.map(Line::raw));
        }
        MMAction::ExecuteOrConfirm(s) => {
            state.set_command_kind(CommandKind::Execute(AfterExecute::ConfirmOnFailure));
            state.set_interrupt(Interrupt::Execute, s);
        }
        MMAction::ExecuteAndQuit(s) => {
            state.set_command_kind(CommandKind::Execute(AfterExecute::FinishOnSuccess));
            state.set_interrupt(Interrupt::Execute, s);
        }
        MMAction::BecomeOr(s) | MMAction::BecomeOrConfirm(s) | MMAction::BecomeOrResume(s) => {
            state.set_command_kind(CommandKind::Execute(AfterExecute::FinishOrConfirm));
            state.set_interrupt(Interrupt::Execute, s);
        }
        MMAction::Transform(payload) => transform(payload, state, ctx),
        MMAction::TransformConfig(payload) => transform_config(payload, state, ctx),
        MMAction::Confirm(payload) => confirm(payload, state, ctx),
        MMAction::Prompt(payload) => prompt(payload, state, ctx),
    }
}

/// Accept: commits an open fm action box, else prints the selection (or the output template) and quits;
/// with nothing selected and no template, quits as "no match".
fn accept(state: &mut MMState<'_, '_>, ctx: &mut ActionContext) {
    let ActionContext {
        render_tx,
        undo_stack,
        redo_stack,
        fm_action,
        fm_notify,
        print_handle,
        output_separator,
        output_template,
        ..
    } = ctx;
    if state.picker_ui.action_visible {
        commit_fm_action(
            state, render_tx, undo_stack, redo_stack, fm_action, *fm_notify,
        );
        return;
    }

    let repeat = |s: String| {
        use std::io::IsTerminal;
        if std::io::stdout().is_terminal() {
            print_handle.push(s);
        } else {
            print!("{}{}", s, output_separator);
        }
    };

    let cwd = std::env::current_dir().ok();
    let selected_items = state.map_selected_to_vec(|_, x| {
        let s = x.to_cow().to_string();
        if let Some(ref dir) = cwd {
            let p = std::path::Path::new(&s);
            if p.is_relative() {
                let full = dir.join(p);
                if full.exists() {
                    return full.to_string_lossy().to_string();
                }
            }
        }
        s
    });
    if state.picker_ui.worker.frecency {
        let store = waymaker::frecency::FrecencyStore::open();
        for item in &selected_items {
            let _ = store.add(item);
        }
    }

    if let Some(template) = output_template {
        crate::formatter::format_cli(state, template, Some(&repeat));
    } else if selected_items.is_empty() {
        // Nothing to output: exit like a failed match so scripts can tell.
        state.request_no_match();
        return;
    } else {
        for item in selected_items {
            repeat(item);
        }
    }

    state.request_finish();
}

/// `Transform(cmd)`: runs the command and sends each line of its output as an action.
fn transform(payload: String, state: &mut MMState<'_, '_>, ctx: &mut ActionContext) {
    let ActionContext { render_tx, .. } = ctx;
    let cmd = format_cli(state, &payload, None);
    if cmd.is_empty() {
        error!("Failed to format transform command: {payload}");
        return;
    }
    let vars = state.make_env_vars();

    let render_tx = render_tx.clone();
    if let Some(contents) = Command::from_script(&cmd)
        .envs(vars)
        .read_to_string()
        ._elog()
    {
        debug!("Transform output:\n{}", contents);

        for line in contents.lines() {
            match Action::<MMAction>::from_str(line) {
                Ok(action) => {
                    let _ = render_tx.send(RenderCommand::Action(action));
                }
                Err(_) => {
                    error!("Failed to parse action from transform output: {}", line);
                }
            }
        }
    }
}

/// `TransformConfig(cmd)`: runs the command and applies its `path=value` lines to the live UI config.
fn transform_config(payload: String, state: &mut MMState<'_, '_>, ctx: &mut ActionContext) {
    let ActionContext { render_tx, .. } = ctx;
    let cmd = format_cli(state, &payload, None);
    if cmd.is_empty() {
        error!("Failed to format transform-config command: {payload}");
        return;
    }
    let vars = state.make_env_vars();

    if let Some(contents) = Command::from_script(&cmd)
        .envs(vars)
        .read_to_string()
        ._elog()
    {
        debug!("TransformConfig output:\n{}", contents);

        let words: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
        match crate::parse::get_pairs(words) {
            Ok(pairs) => {
                let mut partial = PartialRenderConfig::default();
                for (path, val) in pairs {
                    let mut parts = split_on_unescaped_delimiter(&val, "|||");
                    if let Err(e) = crate::parse::try_split_kv(&mut parts, false) {
                        error!("Failed to split KV for {}: {e}", path.join("."));
                        continue;
                    }

                    if let Err(e) = partial.set(path.as_slice(), &parts) {
                        error!("Failed to set partial for {}: {e}", path.join("."));
                    }
                }

                log::debug!("Parsed config update: {partial:?}");

                // Apply the partial to UI components
                state.ui.config.apply(partial.ui);
                state.picker_ui.query.config.apply(partial.query);
                state.picker_ui.results.config.apply(partial.results);
                state.picker_ui.results.status_config.apply(partial.status);
                state.footer_ui.config.apply(partial.footer);
                state.picker_ui.header.config.apply(partial.header);

                if let Some(preview_ui) = state.preview_ui.as_mut() {
                    preview_ui.config.apply(partial.preview);
                }

                let _ = render_tx.send(RenderCommand::Refresh);
            }
            Err(e) => {
                error!("Failed to parse pairs from TransformConfig output: {e}");
            }
        }
    }
}

/// `Confirm(prompt|cmd)`: shows a yes/no box that runs `cmd` on confirm.
fn confirm(payload: String, state: &mut MMState<'_, '_>, ctx: &mut ActionContext) {
    let ActionContext { fm_action, .. } = ctx;
    let (prompt_tmpl, cmd_tmpl) = if let Some((p, c)) = payload.split_once('|') {
        (p.trim(), c.trim())
    } else if let Some((p, c)) = payload.split_once(":::") {
        (p.trim(), c.trim())
    } else {
        (payload.as_str(), "")
    };

    let formatted_prompt = format_cli(state, prompt_tmpl, None);
    let formatted_cmd = format_keeping_input(state, cmd_tmpl);

    *fm_action = Some(FmActionMode::Custom {
        command: formatted_cmd,
    });
    show_styled_action_box(state, &formatted_prompt, "");
}

/// `Prompt(prompt|cmd|initial)`: shows an input box; `cmd` runs with the typed text as `{input}`.
fn prompt(payload: String, state: &mut MMState<'_, '_>, ctx: &mut ActionContext) {
    let ActionContext { fm_action, .. } = ctx;
    let parts: Vec<&str> = payload.split('|').collect();
    let (prompt_tmpl, cmd_tmpl, initial_tmpl) = match parts.len() {
        0 => ("", "", ""),
        1 => (parts[0].trim(), "", ""),
        2 => (parts[0].trim(), parts[1].trim(), ""),
        _ => (parts[0].trim(), parts[1].trim(), parts[2].trim()),
    };

    let formatted_prompt = crate::formatter::format_cli(state, prompt_tmpl, None);
    let formatted_cmd = format_keeping_input(state, cmd_tmpl);
    let formatted_initial = if !initial_tmpl.is_empty() {
        crate::formatter::format_cli(state, initial_tmpl, None)
    } else {
        String::new()
    };

    *fm_action = Some(FmActionMode::Custom {
        command: formatted_cmd,
    });
    show_styled_action_box(state, &formatted_prompt, &formatted_initial);
}

/// Formats a Prompt/Confirm command template now, but leaves `{input}` for the text the user
/// types into the box: format_cli would expand it at once, to the (empty) input of the moment.
fn format_keeping_input(state: &mut MMState<'_, '_>, template: &str) -> String {
    const MARK: &str = "\u{1}wm-prompt-input\u{1}";
    format_cli(state, &template.replace("{input}", MARK), None).replace(MARK, "{input}")
}

impl MMAction {
    /// Validate Bind/PushBind/Unbind/PopBind instructions
    pub fn validate(&self) -> Result<(), StringError> {
        match self {
            MMAction::Bind(s) => {
                let (_trigger, actions) = crate::action::parse_bind_parts(s)?;
                for a in &actions {
                    if let Action::Custom(mm) = a {
                        mm.validate()?;
                    }
                }
            }
            MMAction::PushBind(s) => {
                let (_trigger, a) = crate::action::parse_push_bind_parts(s)?;
                if let Action::Custom(mm) = &a {
                    mm.validate()?;
                }
            }
            MMAction::Unbind(s) | MMAction::PopBind(s) => {
                s.parse::<Trigger>()?;
            }
            _ => {}
        }
        Ok(())
    }
}

pub fn parse_bind_parts(s: &str) -> Result<(Trigger, Actions<MMAction>), StringError> {
    let (trigger, values) = s
        .split_once('=')
        .ok_or_else(|| format!("Expected '=' in Bind({s})"))?;

    let trigger = trigger.trim().parse()?;

    let parts = split_on_unescaped_delimiter(values, "|||");

    let actions = parts
        .iter()
        .map(|p| Action::<MMAction>::from_str(p.trim()))
        .collect::<Result<Vec<_>, _>>()?;

    Ok((trigger, Actions::from_iter(actions)))
}

pub fn parse_push_bind_parts(s: &str) -> Result<(Trigger, Action<MMAction>), StringError> {
    let s = s.trim();
    let (trigger, values) = s
        .split_once('=')
        .ok_or_else(|| format!("Expected '=' in PushBind({s})"))?;

    let trigger = trigger.trim().parse()?;
    let action = Action::<MMAction>::from_str(values.trim())?;

    Ok((trigger, action))
}

enum_from_str_display! {
    MMAction;

    units:
    CycleSort, Accept, ReloadPrev, FmCreateStart, FmDeleteStart, FmRenameStart, FmUnzipStart, FmZipStart, FmYank, FmUnyank, FmCut, FmUncut, FmPaste, FmPasteInto, FmUndo, FmRedo, FmDragDrop, FmTogglePin;


    tuples:
    Bind, Unbind, PushBind, PopBind, ExecuteOrConfirm, ExecuteAndQuit, BecomeOr, BecomeOrConfirm, BecomeOrResume, Transform, TransformConfig, SetStyledPrompt, SetStyledStatus, SetStyledFooter, SetModeIndex, PushHeader, PushFooter, RunPreview, FmSetYankPaths, FmRemoveYankPaths, FmSetCutPaths, FmRemoveCutPaths, FmSetPinPaths, FmSetFlashCopy, FmSetFlashCut, Confirm, Prompt;

    defaults:
    ;

    options:
    SetPrompt, SetHeader, SetFooter, SetStatus, Filtering, ReloadNext;

    lossy:
    ;
}

//------------------------------------------------
macro_rules! enum_from_str_display {
    (
        $enum:ty;
        units: $( $unit:ident ),* $(,)?;
        tuples: $( $tuple:ident ),* $(,)?;
        defaults: $(($default:ident, $default_value:expr)),*;
        options: $($optional:ident),*;
        lossy: $( $lossy:ident ),* ;
    ) => {
        impl std::fmt::Display for $enum {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                use $enum::*;
                match self {
                    $( $unit => write!(f, stringify!($unit)), )*

                    $( $tuple(inner) => write!(f, concat!(stringify!($tuple), "({})"), inner), )*

                    $( $default(inner) => {
                        if *inner == $default_value {
                            write!(f, stringify!($default))
                        } else {
                            write!(f, concat!(stringify!($default), "({})"), inner)
                        }
                    }, )*

                    $( $optional(opt) => {
                        if let Some(inner) = opt {
                            write!(f, concat!(stringify!($optional), "({})"), inner)
                        } else {
                            write!(f, stringify!($optional))
                        }
                    }, )*

                    $( $lossy(inner) => {
                        if inner.is_empty() {
                            write!(f, stringify!($pathbuf))
                        } else {
                            write!(f, concat!(stringify!($lossy), "({})"), std::ffi::OsString::from(inner).to_string_lossy())
                        }
                    }, )*

                    /* ---------- Manually parsed ---------- */
                    Self::ReloadReady(_) => write!(f, "ReloadReady"),
                    /* ------------------------------------- */

                }
            }
        }

        impl std::str::FromStr for $enum {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let (name, data) = if let Some(pos) = s.find('(') {
                    if s.ends_with(')') {
                        (&s[..pos], Some(&s[pos + 1..s.len() - 1]))
                    } else {
                        (s, None)
                    }
                } else {
                    (s, None)
                };

                match name {
                    $( stringify!($unit) => {
                        if data.is_some() {
                            Err(format!("Unexpected data for {}", name))
                        } else {
                            Ok(Self::$unit)
                        }
                    }, )*

                    $( stringify!($tuple) => {
                        let val = data
                        .ok_or_else(|| format!("Missing data for {}", name))?
                        .parse()
                        .map_err(|_| format!("Invalid data for {}", name))?;
                        Ok(Self::$tuple(val))
                    }, )*

                    $( stringify!($lossy) => {
                        let d = match data {
                            Some(val) => val.parse()
                            .map_err(|_| format!("Invalid data for {}", stringify!($lossy)))?,
                            None => Default::default(),
                        };
                        Ok(Self::$lossy(d))
                    }, )*

                    $( stringify!($default) => {
                        let d = match data {
                            Some(val) => val.parse()
                            .map_err(|_| format!("Invalid data for {}", stringify!($default)))?,
                            None => $default_value,
                        };
                        Ok(Self::$default(d))
                    }, )*

                    $( stringify!($optional) => {
                        let d = match data {
                            Some(val) if !val.is_empty() => {
                                Some(val.parse().map_err(|_| format!("Invalid data for {}", stringify!($optional)))?)
                            }
                            _ => None,
                        };
                        Ok(Self::$optional(d))
                    }, )*

                    /* ---------- Manually parsed ---------- */
                    "ReloadReady" => Err("ReloadReady is internal only".into()),
                    /* ------------------------------------- */

                    _ => Err(format!("Unknown action {}", s)),
                }
            }
        }
    };
}
use enum_from_str_display;

use crate::formatter::format_cli;

#[cfg(test)]
mod tests {
    use super::*;
    use waymaker::Action;

    /// Serializes tests that touch process-wide state: the current directory
    /// (chdir actions) and env vars. Run in parallel they
    /// overwrite each other's values.
    static GLOBAL_STATE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[test]
    fn test_parse_styled_footer_keeps_the_whole_template() {
        let tpl = " {cyan,bold:Enter} {darkgray:Launch} {darkgray:│} {cyan,bold:C-h/l} {darkgray:Effort} {white,bold:◑ medium}";
        let action = Action::<MMAction>::from_str(&format!("SetStyledFooter({tpl})")).unwrap();
        assert!(matches!(action, Action::Custom(MMAction::SetStyledFooter(ref s)) if s == tpl));
    }

    /// `HistoryUp`/`HistoryDown` were never implemented: a bind to them must fail to load instead
    /// of doing nothing. `CycleSort` works in every build, not only with `experimental`.
    #[test]
    fn test_unimplemented_actions_are_rejected() {
        assert!(Action::<MMAction>::from_str("HistoryUp").is_err());
        assert!(Action::<MMAction>::from_str("HistoryDown").is_err());
        assert!(matches!(
            Action::<MMAction>::from_str("CycleSort"),
            Ok(Action::Custom(MMAction::CycleSort))
        ));
    }

    #[test]
    fn test_parse_actions() {
        assert!(Action::<MMAction>::from_str("Unbind(QueryChange)").is_ok());
        assert!(Action::<MMAction>::from_str("Filtering(false)").is_ok());
        assert!(Action::<MMAction>::from_str("SetPrompt(rg> )").is_ok());
        assert!(Action::<MMAction>::from_str("Reload").is_ok());

        let bind_inner = match Action::<MMAction>::from_str(
            "Bind(QueryChange = Reload(rg --column --line-number --no-heading --color=always --smart-case \"$FZF_QUERY\"))",
        )
        .unwrap()
        {
            Action::Custom(MMAction::Bind(s)) => s,
            _ => panic!(),
        };

        let (_trigger, actions) = parse_bind_parts(&bind_inner).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Reload(cmd) => assert_eq!(
                cmd,
                "rg --column --line-number --no-heading --color=always --smart-case \"$FZF_QUERY\""
            ),
            _ => panic!(),
        }

        let push_inner = match Action::<MMAction>::from_str("PushBind(ctrl-r = @enter_mm)").unwrap()
        {
            Action::Custom(MMAction::PushBind(s)) => s,
            _ => panic!(),
        };

        let (_trigger, action) = parse_push_bind_parts(&push_inner).unwrap();
        assert_eq!(action, Action::Semantic("enter_mm".into()));
    }

    #[test]
    fn test_reload_next_and_prev_query_preservation() {
        let _global_guard = GLOBAL_STATE_TEST_LOCK.blocking_lock();
        use std::sync::{Arc, Mutex};
        use waymaker::Selector;
        use waymaker::config::*;
        use waymaker::nucleo::Worker;
        use waymaker::render::State;
        use waymaker::ui::{DisplayUI, PickerUI, UI};

        let (bind_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (render_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (controller_tx, _) = tokio::sync::mpsc::unbounded_channel();

        let mut matcher = waymaker::nucleo::nucleo::Matcher::default();
        let worker = Worker::<ConfigMMItem>::new_indexable(["col0"], None);
        let selection_set: Selector<ConfigMMItem, ConfigMMInnerItem> =
            Selector::new(waymaker::nucleo::Indexed::identifier).disabled();

        let mut picker_ui = PickerUI::new(
            ResultsConfig::default(),
            StatusConfig::default(),
            QueryConfig::default(),
            DisplayConfig::default(),
            ActionBoxConfig::default(),
            BreadcrumbConfig::default(),
            &mut matcher,
            worker,
            selection_set,
        );

        let mut ui = UI {
            layout: None,
            area: ratatui::layout::Rect::default(),
            config: UiConfig::default(),
        };
        let mut footer_ui = DisplayUI::new(DisplayConfig::default());
        let mut preview_ui = None;
        let mut state = State::new();

        let mut action_context = ActionContext {
            bind_tx,
            render_tx,
            additional_commands: (
                vec!["cmd0".to_string(), "cmd1".to_string(), "cmd2".to_string()],
                0,
            ),
            output_template: None,
            print_handle: AppendOnly::new(),
            output_separator: "\n".to_string(),
            clipboard: Arc::new(Mutex::new(None)),
            fm_notify: false,
            undo_stack: Arc::new(Mutex::new(Vec::new())),
            redo_stack: Arc::new(Mutex::new(Vec::new())),
            fm_action: None,
            mode_history: std::collections::HashMap::new(),
            last_cwd: std::env::current_dir().ok(),
            nav: Default::default(),
        };

        let mut mm_state = state.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        );

        // 1. Initially on index 0 (normal mode)
        mm_state
            .picker_ui
            .query
            .set(Some("normal_query".to_string()), 12);
        assert_eq!(mm_state.picker_ui.query.input, "normal_query");

        // 2. Cycle to index 1 (cycle mode)
        action_handler(
            MMAction::ReloadNext(None),
            &mut mm_state,
            &mut action_context,
        );
        assert_eq!(action_context.additional_commands.1, 1);
        assert_eq!(mm_state.picker_ui.query.input, "");

        // 3. User types cycle query on index 1
        mm_state
            .picker_ui
            .query
            .set(Some("cycle_query".to_string()), 11);

        // 4. Cycle to index 2
        action_handler(
            MMAction::ReloadNext(None),
            &mut mm_state,
            &mut action_context,
        );
        assert_eq!(action_context.additional_commands.1, 2);
        assert_eq!(mm_state.picker_ui.query.input, "");

        // 5. Cycle back to index 0 (normal mode)
        action_handler(
            MMAction::ReloadNext(None),
            &mut mm_state,
            &mut action_context,
        );
        assert_eq!(action_context.additional_commands.1, 0);
        assert_eq!(mm_state.picker_ui.query.input, "normal_query");

        // 6. Cycle to index 1 (cycle mode)
        action_handler(
            MMAction::ReloadNext(None),
            &mut mm_state,
            &mut action_context,
        );
        assert_eq!(action_context.additional_commands.1, 1);
        assert_eq!(mm_state.picker_ui.query.input, "cycle_query");

        // 7. Cycle backwards (ReloadPrev) to index 0 (normal mode)
        action_handler(MMAction::ReloadPrev, &mut mm_state, &mut action_context);
        assert_eq!(action_context.additional_commands.1, 0);
        assert_eq!(mm_state.picker_ui.query.input, "normal_query");

        // 8. Test nav mode focus behavior
        mm_state.ui.config.nav.active = true;
        mm_state.set_focus(waymaker::render::Focus::Results);
        action_handler(
            MMAction::ReloadNext(None),
            &mut mm_state,
            &mut action_context,
        );
        let mode_0_hist = action_context.mode_history.get(&0).unwrap();
        assert_eq!(mode_0_hist.focus, waymaker::render::Focus::Results);
        assert_eq!(mm_state.focus(), waymaker::render::Focus::Input);

        // Switch to mode 2 (Bookmarks) - when no prior nav_mode history, should default to Focus::Results
        action_context.mode_history.remove(&2);
        action_handler(
            MMAction::ReloadNext(Some(2)),
            &mut mm_state,
            &mut action_context,
        );
        assert_eq!(action_context.additional_commands.1, 2);
        assert_eq!(mm_state.focus(), waymaker::render::Focus::Results);

        // Switch back to mode 0 - should restore Nav mode (Focus::Results)
        action_handler(
            MMAction::ReloadNext(Some(0)),
            &mut mm_state,
            &mut action_context,
        );
        assert_eq!(action_context.additional_commands.1, 0);
        assert_eq!(mm_state.focus(), waymaker::render::Focus::Results);
    }

    #[test]
    fn test_fm_toggle_pin() {
        crate::test_support::isolate_frecency();
        let _global_guard = GLOBAL_STATE_TEST_LOCK.blocking_lock();
        use std::sync::{Arc, Mutex};
        use waymaker::Selector;
        use waymaker::config::*;
        use waymaker::nucleo::Worker;
        use waymaker::render::State;
        use waymaker::ui::{DisplayUI, PickerUI, UI};

        let (bind_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (render_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (controller_tx, _) = tokio::sync::mpsc::unbounded_channel();

        let mut matcher = waymaker::nucleo::nucleo::Matcher::default();
        let worker = Worker::<ConfigMMItem>::new_indexable(["col0"], None);
        let selection_set: Selector<ConfigMMItem, ConfigMMInnerItem> =
            Selector::new(waymaker::nucleo::Indexed::identifier).disabled();

        let mut picker_ui = PickerUI::new(
            ResultsConfig::default(),
            StatusConfig::default(),
            QueryConfig::default(),
            DisplayConfig::default(),
            ActionBoxConfig::default(),
            BreadcrumbConfig::default(),
            &mut matcher,
            worker,
            selection_set,
        );

        let mut ui = UI {
            layout: None,
            area: ratatui::layout::Rect::default(),
            config: UiConfig::default(),
        };
        let mut footer_ui = DisplayUI::new(DisplayConfig::default());
        let mut preview_ui = None;
        let mut state = State::new();

        let mut action_context = ActionContext {
            bind_tx,
            render_tx,
            additional_commands: (vec!["cmd0".to_string()], 0),
            output_template: None,
            print_handle: AppendOnly::new(),
            output_separator: "\n".to_string(),
            clipboard: Arc::new(Mutex::new(None)),
            fm_notify: false,
            undo_stack: Arc::new(Mutex::new(Vec::new())),
            redo_stack: Arc::new(Mutex::new(Vec::new())),
            fm_action: None,
            mode_history: std::collections::HashMap::new(),
            last_cwd: std::env::current_dir().ok(),
            nav: Default::default(),
        };

        let mut mm_state = state.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        );

        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let store = waymaker::frecency::FrecencyStore::open();
        let initial_state = store.is_pinned(&cwd);

        // 1st Toggle
        action_handler(MMAction::FmTogglePin, &mut mm_state, &mut action_context);
        let after_1st = store.is_pinned(&cwd);
        assert_eq!(after_1st, !initial_state);

        // 2nd Toggle
        action_handler(MMAction::FmTogglePin, &mut mm_state, &mut action_context);
        let after_2nd = store.is_pinned(&cwd);
        assert_eq!(after_2nd, initial_state);
    }

    #[tokio::test]
    async fn test_fm_paste_into_and_flash() {
        crate::test_support::isolate_frecency();
        let _global_guard = GLOBAL_STATE_TEST_LOCK.lock().await;
        use std::sync::{Arc, Mutex};
        use tokio::sync::mpsc;
        use waymaker::nucleo::injector::Injector;
        use waymaker::preview::AppendOnly;
        use waymaker::render::State;
        use waymaker::ui::UI;

        let (mut mm, injector, _guard) = waymaker::ConfigMatchmaker::new_from_config(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );

        let temp_dir = std::env::temp_dir().join(format!("test_paste_into_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let sub_dir = temp_dir.join("target_folder");
        std::fs::create_dir_all(&sub_dir).unwrap();
        let source_file = temp_dir.join("source.txt");
        std::fs::write(&source_file, "hello world").unwrap();

        injector
            .push((None, sub_dir.to_string_lossy().as_ref().into()))
            .unwrap();
        mm.worker.nucleo.tick(10);

        let mut state_obj = State::new();
        let mut tui = waymaker::tui::Tui::headless(
            waymaker::config::TerminalConfig::default(),
            ratatui::layout::Rect::new(0, 0, 80, 24),
        )
        .unwrap();
        let mut matcher =
            waymaker::nucleo::nucleo::Matcher::new(waymaker::nucleo::nucleo::Config::DEFAULT);

        let hidden_columns = vec![false];
        let (mut ui, mut picker_ui, mut footer_ui, mut preview_ui) = UI::new(
            mm.render_config,
            &mut matcher,
            mm.worker,
            mm.selector,
            None,
            &mut tui,
            hidden_columns,
        );

        let (bind_tx, _) = mpsc::unbounded_channel();
        let (render_tx, _) = mpsc::unbounded_channel();
        let (controller_tx, _) = mpsc::unbounded_channel();

        let clipboard = Arc::new(Mutex::new(Some(crate::fm::FmClipboard {
            items: vec![source_file.clone()],
            op: crate::fm::ClipOp::Copy,
        })));
        let undo_stack = Arc::new(Mutex::new(Vec::new()));
        let redo_stack = Arc::new(Mutex::new(Vec::new()));

        let mut action_context = ActionContext {
            bind_tx,
            render_tx,
            additional_commands: (vec!["cmd0".to_string()], 0),
            output_template: None,
            print_handle: AppendOnly::new(),
            output_separator: "\n".to_string(),
            clipboard: clipboard.clone(),
            fm_notify: false,
            undo_stack: undo_stack.clone(),
            redo_stack: redo_stack.clone(),
            fm_action: None,
            mode_history: std::collections::HashMap::new(),
            last_cwd: Some(temp_dir.clone()),
            nav: Default::default(),
        };

        let mut mm_state = state_obj.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        );

        // Test FmPasteInto
        action_handler(MMAction::FmPasteInto, &mut mm_state, &mut action_context);

        let pasted_file = sub_dir.join("source.txt");
        assert!(
            pasted_file.exists(),
            "Pasted file should exist inside target_folder"
        );
        assert_eq!(
            std::fs::read_to_string(&pasted_file).unwrap(),
            "hello world"
        );
        assert_eq!(
            undo_stack.lock().unwrap().len(),
            1,
            "Undo stack should have 1 item"
        );
        assert!(
            clipboard.lock().unwrap().is_none(),
            "Clipboard should be cleared after paste"
        );
        assert!(
            mm_state.picker_ui.action_visible,
            "Action box should be visible for info notification"
        );

        // Test Flash action
        action_handler(
            MMAction::FmSetFlashCopy("target_folder".to_string()),
            &mut mm_state,
            &mut action_context,
        );
        let flash_op = mm_state
            .picker_ui
            .results
            .get_flash_op("target_folder", &temp_dir);
        assert_eq!(flash_op, Some(waymaker::ui::results::FlashOp::Copy));

        // Test Undo
        action_handler(MMAction::FmUndo, &mut mm_state, &mut action_context);
        assert!(
            !pasted_file.exists(),
            "Pasted file should be removed after undo"
        );
        assert!(
            clipboard.lock().unwrap().is_some(),
            "Clipboard should be restored after undo"
        );
        assert!(
            mm_state.picker_ui.action_visible,
            "Action box should be visible with undo notification"
        );

        // Test exact item counts in yank_paths and cut_paths
        action_handler(
            MMAction::FmSetYankPaths("folder/\nfile.txt".to_string()),
            &mut mm_state,
            &mut action_context,
        );
        assert_eq!(
            mm_state.picker_ui.results.yank_paths.len(),
            2,
            "yank_paths should contain exactly 2 items (1 for folder/ and 1 for file.txt)"
        );

        action_handler(
            MMAction::FmRemoveYankPaths("folder/".to_string()),
            &mut mm_state,
            &mut action_context,
        );
        assert_eq!(
            mm_state.picker_ui.results.yank_paths.len(),
            1,
            "yank_paths should contain exactly 1 item after removing folder/"
        );
        assert!(mm_state.picker_ui.results.yank_paths.contains("file.txt"));

        action_handler(
            MMAction::FmSetCutPaths("single_folder/".to_string()),
            &mut mm_state,
            &mut action_context,
        );
        assert_eq!(
            mm_state.picker_ui.results.cut_paths.len(),
            1,
            "cut_paths should contain exactly 1 item for single folder"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_fm_batch_paste_and_undo_clipboard_restoration() {
        crate::test_support::isolate_frecency();
        let _global_guard = GLOBAL_STATE_TEST_LOCK.lock().await;
        use std::sync::{Arc, Mutex};
        use tokio::sync::mpsc;
        use waymaker::nucleo::injector::Injector;
        use waymaker::preview::AppendOnly;
        use waymaker::render::State;
        use waymaker::ui::UI;

        let (mut mm, injector, _guard) = waymaker::ConfigMatchmaker::new_from_config(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );

        let temp_dir =
            std::env::temp_dir().join(format!("test_batch_paste_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file1 = temp_dir.join("file1.txt");
        let file2 = temp_dir.join("file2.txt");
        let target_dir = temp_dir.join("dest_dir");
        std::fs::write(&file1, "content 1").unwrap();
        std::fs::write(&file2, "content 2").unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();

        injector
            .push((None, target_dir.to_string_lossy().as_ref().into()))
            .unwrap();
        mm.worker.nucleo.tick(10);

        let mut state_obj = State::new();
        let mut tui = waymaker::tui::Tui::headless(
            waymaker::config::TerminalConfig::default(),
            ratatui::layout::Rect::new(0, 0, 80, 24),
        )
        .unwrap();
        let mut matcher =
            waymaker::nucleo::nucleo::Matcher::new(waymaker::nucleo::nucleo::Config::DEFAULT);

        let hidden_columns = vec![false];
        let (mut ui, mut picker_ui, mut footer_ui, mut preview_ui) = UI::new(
            mm.render_config,
            &mut matcher,
            mm.worker,
            mm.selector,
            None,
            &mut tui,
            hidden_columns,
        );

        let (bind_tx, _) = mpsc::unbounded_channel();
        let (render_tx, _) = mpsc::unbounded_channel();
        let (controller_tx, _) = mpsc::unbounded_channel();

        let clipboard = Arc::new(Mutex::new(Some(crate::fm::FmClipboard {
            items: vec![file1.clone(), file2.clone()],
            op: crate::fm::ClipOp::Copy,
        })));
        let undo_stack = Arc::new(Mutex::new(Vec::new()));
        let redo_stack = Arc::new(Mutex::new(Vec::new()));

        let mut action_context = ActionContext {
            bind_tx,
            render_tx,
            additional_commands: (vec!["cmd0".to_string()], 0),
            output_template: None,
            print_handle: AppendOnly::new(),
            output_separator: "\n".to_string(),
            clipboard: clipboard.clone(),
            fm_notify: true,
            undo_stack: undo_stack.clone(),
            redo_stack: redo_stack.clone(),
            fm_action: None,
            mode_history: std::collections::HashMap::new(),
            last_cwd: Some(temp_dir.clone()),
            nav: Default::default(),
        };

        let mut mm_state = state_obj.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        );

        mm_state
            .picker_ui
            .results
            .yank_paths
            .insert("file1.txt".to_string());
        mm_state
            .picker_ui
            .results
            .yank_paths
            .insert("file2.txt".to_string());

        // Execute Paste Into target_dir
        action_handler(MMAction::FmPasteInto, &mut mm_state, &mut action_context);

        let pasted1 = target_dir.join("file1.txt");
        let pasted2 = target_dir.join("file2.txt");
        assert!(pasted1.exists(), "pasted1 should exist");
        assert!(pasted2.exists(), "pasted2 should exist");
        assert!(
            clipboard.lock().unwrap().is_none(),
            "clipboard cleared after paste"
        );
        assert!(
            mm_state.picker_ui.results.yank_paths.is_empty(),
            "yank_paths cleared after paste"
        );
        assert!(
            mm_state.picker_ui.action_visible,
            "action box visible for feedback"
        );

        // Execute Undo
        action_handler(MMAction::FmUndo, &mut mm_state, &mut action_context);

        assert!(!pasted1.exists(), "pasted1 should be removed on undo");
        assert!(!pasted2.exists(), "pasted2 should be removed on undo");
        assert!(
            clipboard.lock().unwrap().is_some(),
            "clipboard restored on undo"
        );
        assert_eq!(clipboard.lock().unwrap().as_ref().unwrap().items.len(), 2);
        assert_eq!(
            mm_state.picker_ui.results.yank_paths.len(),
            2,
            "yank_paths restored on undo"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_fm_paste_into_chdir_and_undo_return_and_redo() {
        crate::test_support::isolate_frecency();
        let _global_guard = GLOBAL_STATE_TEST_LOCK.lock().await;
        use std::sync::{Arc, Mutex};
        use tokio::sync::mpsc;
        use waymaker::action::Action;
        use waymaker::message::RenderCommand;
        use waymaker::nucleo::injector::Injector;
        use waymaker::preview::AppendOnly;
        use waymaker::render::State;
        use waymaker::ui::UI;

        let (mut mm, injector, _guard) = waymaker::ConfigMatchmaker::new_from_config(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );

        let temp_dir =
            std::env::temp_dir().join(format!("test_paste_into_chdir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let cwd = std::env::current_dir().unwrap_or_default();

        let src_file = temp_dir.join("payload.txt");
        let dest_folder = temp_dir.join("target_subfolder");
        std::fs::write(&src_file, "payload data").unwrap();
        std::fs::create_dir_all(&dest_folder).unwrap();

        injector
            .push((None, dest_folder.to_string_lossy().as_ref().into()))
            .unwrap();
        mm.worker.nucleo.tick(10);

        let mut state_obj = State::new();
        let mut tui = waymaker::tui::Tui::headless(
            waymaker::config::TerminalConfig::default(),
            ratatui::layout::Rect::new(0, 0, 80, 24),
        )
        .unwrap();
        let mut matcher =
            waymaker::nucleo::nucleo::Matcher::new(waymaker::nucleo::nucleo::Config::DEFAULT);

        let hidden_columns = vec![false];
        let (mut ui, mut picker_ui, mut footer_ui, mut preview_ui) = UI::new(
            mm.render_config,
            &mut matcher,
            mm.worker,
            mm.selector,
            None,
            &mut tui,
            hidden_columns,
        );

        let (bind_tx, _) = mpsc::unbounded_channel();
        let (render_tx, mut render_rx) = mpsc::unbounded_channel();
        let (controller_tx, _) = mpsc::unbounded_channel();

        let clipboard = Arc::new(Mutex::new(Some(crate::fm::FmClipboard {
            items: vec![src_file.clone()],
            op: crate::fm::ClipOp::Copy,
        })));
        let undo_stack = Arc::new(Mutex::new(Vec::new()));
        let redo_stack = Arc::new(Mutex::new(Vec::new()));

        let mut action_context = ActionContext {
            bind_tx,
            render_tx,
            additional_commands: (vec!["cmd0".to_string()], 0),
            output_template: None,
            print_handle: AppendOnly::new(),
            output_separator: "\n".to_string(),
            clipboard: clipboard.clone(),
            fm_notify: true,
            undo_stack: undo_stack.clone(),
            redo_stack: redo_stack.clone(),
            fm_action: None,
            mode_history: std::collections::HashMap::new(),
            last_cwd: Some(cwd.clone()),
            nav: Default::default(),
        };

        let mut mm_state = state_obj.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        );

        // 1. Execute FmPasteInto
        action_handler(MMAction::FmPasteInto, &mut mm_state, &mut action_context);

        let pasted = dest_folder.join("payload.txt");
        assert!(pasted.exists(), "Item should be copied into dest_folder");
        assert!(
            mm_state.picker_ui.action_visible,
            "Info box should be visible on paste into"
        );
        let paste_prompt = mm_state.picker_ui.action.prompt();
        assert_eq!(paste_prompt.spans.len(), 1);
        assert_eq!(
            paste_prompt.spans[0].style.fg,
            Some(ratatui::style::Color::Cyan)
        );
        assert!(paste_prompt.spans[0].content.contains("Pasted 1 item into"));

        // Check that undo stack recorded previous_dir and target_dir
        {
            let u = undo_stack.lock().unwrap();
            assert_eq!(u.len(), 1);
            if let crate::fm::UndoAction::Paste {
                previous_dir,
                target_dir,
                ..
            } = &u[0]
            {
                assert_eq!(previous_dir.as_ref(), Some(&cwd));
                assert_eq!(target_dir.as_ref(), Some(&dest_folder));
            } else {
                panic!("Expected UndoAction::Paste");
            }
        }

        // Drain render_rx and check that ChDir to dest_folder was sent
        let mut chdir_dest_found = false;
        while let Ok(cmd) = render_rx.try_recv() {
            if let RenderCommand::Action(Action::ChDir(dir)) = cmd
                && dir == dest_folder.to_string_lossy()
            {
                chdir_dest_found = true;
            }
        }
        assert!(
            chdir_dest_found,
            "FmPasteInto must send Action::ChDir to dest_folder"
        );

        // Check that the nav target item was set to the pasted file name
        assert_eq!(
            action_context.nav.lock().unwrap().target_item.as_deref(),
            Some("payload.txt"),
            "target_item should highlight pasted item inside dest_folder"
        );

        // 2. Execute FmUndo
        action_handler(MMAction::FmUndo, &mut mm_state, &mut action_context);
        assert!(!pasted.exists(), "Pasted item should be deleted on undo");
        assert!(
            mm_state.picker_ui.action_visible,
            "Info box should be visible on undo"
        );
        let undo_prompt = mm_state.picker_ui.action.prompt();
        assert_eq!(undo_prompt.spans.len(), 1);
        assert_eq!(
            undo_prompt.spans[0].style.fg,
            Some(ratatui::style::Color::Yellow)
        );
        assert!(undo_prompt.spans[0].content.contains("Undone copy"));

        // Drain render_rx and check that ChDir to previous_dir (cwd) was sent
        let mut chdir_prev_found = false;
        while let Ok(cmd) = render_rx.try_recv() {
            if let RenderCommand::Action(Action::ChDir(dir)) = cmd
                && dir == cwd.to_string_lossy()
            {
                chdir_prev_found = true;
            }
        }
        assert!(
            chdir_prev_found,
            "FmUndo must send Action::ChDir to previous_dir"
        );

        // Check that the nav target item was set to dest_folder name so it is selected back in previous_dir
        assert_eq!(
            action_context.nav.lock().unwrap().target_item.as_deref(),
            Some("target_subfolder"),
            "target_item should highlight target_subfolder when returning to parent on undo"
        );

        // 3. Execute FmRedo
        action_handler(MMAction::FmRedo, &mut mm_state, &mut action_context);
        assert!(pasted.exists(), "Pasted item should be restored on redo");
        assert!(
            mm_state.picker_ui.action_visible,
            "Info box should be visible on redo"
        );
        let redo_prompt = mm_state.picker_ui.action.prompt();
        assert_eq!(redo_prompt.spans.len(), 1);
        assert_eq!(
            redo_prompt.spans[0].style.fg,
            Some(ratatui::style::Color::Cyan)
        );
        assert!(redo_prompt.spans[0].content.contains("Redone"));

        let mut chdir_redo_found = false;
        while let Ok(cmd) = render_rx.try_recv() {
            if let RenderCommand::Action(Action::ChDir(dir)) = cmd
                && dir == dest_folder.to_string_lossy()
            {
                chdir_redo_found = true;
            }
        }
        assert!(
            chdir_redo_found,
            "FmRedo must send Action::ChDir to dest_folder"
        );

        assert_eq!(
            action_context.nav.lock().unwrap().target_item.as_deref(),
            Some("payload.txt"),
            "target_item should highlight pasted item inside dest_folder on redo"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_fm_paste_into_cut_and_undo_return_and_redo() {
        crate::test_support::isolate_frecency();
        let _global_guard = GLOBAL_STATE_TEST_LOCK.lock().await;
        use std::sync::{Arc, Mutex};
        use tokio::sync::mpsc;
        use waymaker::action::Action;
        use waymaker::message::RenderCommand;
        use waymaker::nucleo::injector::Injector;
        use waymaker::preview::AppendOnly;
        use waymaker::render::State;
        use waymaker::ui::UI;

        let (mut mm, injector, _guard) = waymaker::ConfigMatchmaker::new_from_config(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );

        let temp_dir =
            std::env::temp_dir().join(format!("test_paste_into_cut_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let cwd = std::env::current_dir().unwrap_or_default();

        let src_file = temp_dir.join("cut_payload.txt");
        let dest_folder = temp_dir.join("cut_target_subfolder");
        std::fs::write(&src_file, "cut payload data").unwrap();
        std::fs::create_dir_all(&dest_folder).unwrap();

        injector
            .push((None, dest_folder.to_string_lossy().as_ref().into()))
            .unwrap();
        mm.worker.nucleo.tick(10);

        let mut state_obj = State::new();
        let mut tui = waymaker::tui::Tui::headless(
            waymaker::config::TerminalConfig::default(),
            ratatui::layout::Rect::new(0, 0, 80, 24),
        )
        .unwrap();
        let mut matcher =
            waymaker::nucleo::nucleo::Matcher::new(waymaker::nucleo::nucleo::Config::DEFAULT);

        let hidden_columns = vec![false];
        let (mut ui, mut picker_ui, mut footer_ui, mut preview_ui) = UI::new(
            mm.render_config,
            &mut matcher,
            mm.worker,
            mm.selector,
            None,
            &mut tui,
            hidden_columns,
        );

        let (bind_tx, _) = mpsc::unbounded_channel();
        let (render_tx, mut render_rx) = mpsc::unbounded_channel();
        let (controller_tx, _) = mpsc::unbounded_channel();

        let clipboard = Arc::new(Mutex::new(Some(crate::fm::FmClipboard {
            items: vec![src_file.clone()],
            op: crate::fm::ClipOp::Cut,
        })));
        let undo_stack = Arc::new(Mutex::new(Vec::new()));
        let redo_stack = Arc::new(Mutex::new(Vec::new()));

        let mut action_context = ActionContext {
            bind_tx,
            render_tx,
            additional_commands: (vec!["cmd0".to_string()], 0),
            output_template: None,
            print_handle: AppendOnly::new(),
            output_separator: "\n".to_string(),
            clipboard: clipboard.clone(),
            fm_notify: true,
            undo_stack: undo_stack.clone(),
            redo_stack: redo_stack.clone(),
            fm_action: None,
            mode_history: std::collections::HashMap::new(),
            last_cwd: Some(cwd.clone()),
            nav: Default::default(),
        };

        let mut mm_state = state_obj.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        );

        // 1. Execute FmPasteInto (Cut)
        action_handler(MMAction::FmPasteInto, &mut mm_state, &mut action_context);

        let moved_file = dest_folder.join("cut_payload.txt");
        assert!(moved_file.exists(), "Item should be moved into dest_folder");
        assert!(
            !src_file.exists(),
            "Source file should no longer exist at original path"
        );

        // Drain render_rx and check that ChDir to dest_folder was sent
        let mut chdir_dest_found = false;
        while let Ok(cmd) = render_rx.try_recv() {
            if let RenderCommand::Action(Action::ChDir(dir)) = cmd
                && dir == dest_folder.to_string_lossy()
            {
                chdir_dest_found = true;
            }
        }
        assert!(
            chdir_dest_found,
            "FmPasteInto must send Action::ChDir to dest_folder"
        );
        assert_eq!(
            action_context.nav.lock().unwrap().target_item.as_deref(),
            Some("cut_payload.txt"),
            "target_item should highlight moved item inside dest_folder"
        );

        // 2. Execute FmUndo
        action_handler(MMAction::FmUndo, &mut mm_state, &mut action_context);
        assert!(
            !moved_file.exists(),
            "Moved file should no longer be in dest_folder"
        );
        assert!(
            src_file.exists(),
            "Source file should be moved back to original path"
        );

        let mut chdir_prev_found = false;
        while let Ok(cmd) = render_rx.try_recv() {
            if let RenderCommand::Action(Action::ChDir(dir)) = cmd
                && dir == cwd.to_string_lossy()
            {
                chdir_prev_found = true;
            }
        }
        assert!(
            chdir_prev_found,
            "FmUndo must send Action::ChDir to previous_dir"
        );
        assert_eq!(
            action_context.nav.lock().unwrap().target_item.as_deref(),
            Some("cut_target_subfolder"),
            "target_item should highlight cut_target_subfolder when returning to parent on undo"
        );

        // 3. Execute FmRedo
        action_handler(MMAction::FmRedo, &mut mm_state, &mut action_context);
        assert!(
            moved_file.exists(),
            "File should be moved into dest_folder again on redo"
        );
        assert!(
            !src_file.exists(),
            "Source file should not exist at original path on redo"
        );

        let mut chdir_redo_found = false;
        while let Ok(cmd) = render_rx.try_recv() {
            if let RenderCommand::Action(Action::ChDir(dir)) = cmd
                && dir == dest_folder.to_string_lossy()
            {
                chdir_redo_found = true;
            }
        }
        assert!(
            chdir_redo_found,
            "FmRedo must send Action::ChDir to dest_folder"
        );
        assert_eq!(
            action_context.nav.lock().unwrap().target_item.as_deref(),
            Some("cut_payload.txt"),
            "target_item should highlight moved item inside dest_folder on redo"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_abbreviate_target_dir_display() {
        // 1 segment relative
        assert_eq!(
            abbreviate_target_dir_display("target_folder"),
            "./target_folder/"
        );
        // 1 segment relative with leading ./ and trailing /
        assert_eq!(
            abbreviate_target_dir_display("./target_folder/"),
            "./target_folder/"
        );
        // 2 segments relative
        assert_eq!(abbreviate_target_dir_display("sub/folder"), "./sub/folder/");
        // 3 segments relative
        assert_eq!(abbreviate_target_dir_display("a/b/c"), "./a/b/c/");
        // 4 segments relative -> abbreviate to .../parent2/parent1/current
        assert_eq!(abbreviate_target_dir_display("a/b/c/d"), ".../b/c/d");
        // Deep relative
        assert_eq!(abbreviate_target_dir_display("a/b/c/d/e/f"), ".../d/e/f");

        // 1 segment absolute
        assert_eq!(abbreviate_target_dir_display("/folder"), "/folder/");
        // 2 segments absolute
        assert_eq!(abbreviate_target_dir_display("/usr/bin"), "/usr/bin/");
        // 3 segments absolute
        assert_eq!(
            abbreviate_target_dir_display("/home/user/docs"),
            "/home/user/docs/"
        );
        // 4 segments absolute -> abbreviate
        assert_eq!(
            abbreviate_target_dir_display("/home/user/code/project"),
            ".../user/code/project"
        );
        // Deep absolute path
        assert_eq!(
            abbreviate_target_dir_display("/home/fecavmi/dev/github/matchmaker/matchmaker-cli/src"),
            ".../matchmaker/matchmaker-cli/src"
        );
    }
}
