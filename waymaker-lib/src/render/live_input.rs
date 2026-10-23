//! Live-input passthrough and prompt-input command rewriting.

use crate::action::{Action, ActionExt};
use crate::message::RenderCommand;

pub(super) fn normalize_live_key(key: &str) -> String {
    let lower = key.to_ascii_lowercase();
    match lower.as_str() {
        "enter" | "return" => "Enter".to_string(),
        "backspace" | "bspace" => "BSpace".to_string(),
        "tab" => "Tab".to_string(),
        "btab" | "backtab" | "shift-tab" => "BTab".to_string(),
        "delete" | "dc" => "DC".to_string(),
        "up" => "Up".to_string(),
        "down" => "Down".to_string(),
        "left" => "Left".to_string(),
        "right" => "Right".to_string(),
        "space" | " " => "Space".to_string(),
        "pageup" | "page_up" => "PageUp".to_string(),
        "pagedown" | "page_down" => "PageDown".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        k if k.starts_with("ctrl-") => {
            let rest = &k[5..];
            format!("C-{rest}")
        }
        k if k.starts_with("alt-") => {
            let rest = &k[4..];
            format!("M-{rest}")
        }
        _ => key.to_string(),
    }
}

#[inline]
pub fn quote_live_token(token: &str) -> String {
    if token == ";" {
        r"'\;'".to_string()
    } else {
        let escaped = token.replace('\'', r"'\''");
        format!("'{escaped}'")
    }
}

#[inline]
pub(super) fn apply_live_input<A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    live_input_active: bool,
) {
    if !live_input_active {
        return;
    }

    let mut out = Vec::with_capacity(buffer.len());
    let mut current_text_batch = String::new();
    let mut key_batch: Vec<String> = Vec::new();

    let flush_batch = |key_batch: &mut Vec<String>,
                       current_text_batch: &mut String,
                       out: &mut Vec<RenderCommand<A>>| {
        if !current_text_batch.is_empty() {
            key_batch.push(std::mem::take(current_text_batch));
        }
        if !key_batch.is_empty() {
            let keys = std::mem::take(key_batch);
            let formatted_keys = keys
                .into_iter()
                .map(|k| quote_live_token(&k))
                .collect::<Vec<_>>()
                .join(" ");
            out.push(RenderCommand::Action(Action::LiveInputKey(formatted_keys)));
        }
    };

    for cmd in buffer.drain(..) {
        match cmd {
            RenderCommand::Tick => {
                flush_batch(&mut key_batch, &mut current_text_batch, &mut out);
                out.push(RenderCommand::Tick);
            }
            RenderCommand::KeyAction { ref key, .. } if key.eq_ignore_ascii_case("esc") => {
                flush_batch(&mut key_batch, &mut current_text_batch, &mut out);
                out.push(RenderCommand::Action(Action::Cancel));
            }
            RenderCommand::Action(Action::Quit(_))
            | RenderCommand::Action(Action::Cancel)
            | RenderCommand::KeyAction {
                action: Action::Quit(_) | Action::Cancel,
                ..
            } => {
                flush_batch(&mut key_batch, &mut current_text_batch, &mut out);
                out.push(RenderCommand::Action(Action::Cancel));
            }
            RenderCommand::Action(Action::Char(c))
            | RenderCommand::KeyAction {
                action: Action::Char(c),
                ..
            } => {
                if c == ' ' {
                    if !current_text_batch.is_empty() {
                        key_batch.push(std::mem::take(&mut current_text_batch));
                    }
                    key_batch.push("Space".to_string());
                } else if c == ';' {
                    if !current_text_batch.is_empty() {
                        key_batch.push(std::mem::take(&mut current_text_batch));
                    }
                    key_batch.push(";".to_string());
                } else if !c.is_ascii_control() {
                    current_text_batch.push(c);
                }
            }
            RenderCommand::KeyAction { ref key, .. }
                if key == " " || key.eq_ignore_ascii_case("space") =>
            {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push("Space".to_string());
            }
            RenderCommand::KeyAction { ref key, .. } if key == ";" => {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push(";".to_string());
            }
            RenderCommand::KeyAction { ref key, .. } => {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push(normalize_live_key(key));
            }
            RenderCommand::Action(Action::Accept) => {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push("Enter".to_string());
            }
            RenderCommand::Action(Action::DeleteChar) => {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push("BSpace".to_string());
            }
            RenderCommand::Action(Action::DeleteNextChar) => {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push("DC".to_string());
            }
            RenderCommand::Action(Action::Up(_)) => {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push("Up".to_string());
            }
            RenderCommand::Action(Action::Down(_)) => {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push("Down".to_string());
            }
            RenderCommand::Action(Action::ForwardChar) => {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push("Right".to_string());
            }
            RenderCommand::Action(Action::BackwardChar) => {
                if !current_text_batch.is_empty() {
                    key_batch.push(std::mem::take(&mut current_text_batch));
                }
                key_batch.push("Left".to_string());
            }
            other => {
                flush_batch(&mut key_batch, &mut current_text_batch, &mut out);
                out.push(other);
            }
        }
    }

    flush_batch(&mut key_batch, &mut current_text_batch, &mut out);
    *buffer = out;
}

#[inline]
pub(super) fn apply_prompt_input<A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    prompt_input_active: bool,
) {
    if !prompt_input_active {
        return;
    }

    let mut out = Vec::with_capacity(buffer.len());

    for cmd in buffer.drain(..) {
        match cmd {
            RenderCommand::Tick => out.push(RenderCommand::Tick),
            RenderCommand::KeyAction { key, .. } if key.eq_ignore_ascii_case("esc") => {
                out.push(RenderCommand::Action(Action::Cancel));
            }
            RenderCommand::KeyAction { key, .. } if key.eq_ignore_ascii_case("enter") => {
                out.push(RenderCommand::Action(Action::Accept));
            }
            RenderCommand::KeyAction { key, .. } if key.eq_ignore_ascii_case("backspace") => {
                out.push(RenderCommand::Action(Action::DeleteChar));
            }
            RenderCommand::Action(Action::Quit(_))
            | RenderCommand::Action(Action::Cancel)
            | RenderCommand::KeyAction {
                action: Action::Quit(_) | Action::Cancel,
                ..
            } => {
                out.push(RenderCommand::Action(Action::Cancel));
            }
            RenderCommand::Action(Action::Accept)
            | RenderCommand::KeyAction {
                action: Action::Accept,
                ..
            } => {
                out.push(RenderCommand::Action(Action::Accept));
            }
            RenderCommand::Action(a) | RenderCommand::KeyAction { action: a, .. } => {
                out.push(RenderCommand::Action(a));
            }
            other => {
                out.push(other);
            }
        }
    }

    *buffer = out;
}
