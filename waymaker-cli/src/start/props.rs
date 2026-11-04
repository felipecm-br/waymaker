use super::*;

/// `WM_BORDER_GRADIENT` (a gradient preset or hex stops, e.g. exported by a
/// session-wide toggle) turns on a rounded outer frame painted with that
/// gradient; `WM_BORDER_TITLE` labels it when the config has no title.
/// Applied before CLI overrides, which still win.
pub(super) fn apply_border_gradient_env(border: &mut waymaker::config::BorderSetting) {
    let Ok(gradient) = std::env::var("WM_BORDER_GRADIENT") else {
        return;
    };
    let gradient = gradient.trim();
    if waymaker::ui::gradient::parse_stops(gradient).is_none() {
        return;
    }
    border.gradient = gradient.to_string();
    if border.r#type.is_none() {
        border.r#type = Some(ratatui::widgets::BorderType::Rounded);
    }
    if border.sides.is_none_or(|sides| sides.is_empty()) {
        border.sides = Some(ratatui::widgets::Borders::ALL);
    }
    if border.title.is_empty()
        && let Ok(title) = std::env::var("WM_BORDER_TITLE")
    {
        border.title = title;
    }
}

pub(super) fn parse_border_type(s: &str) -> ratatui::widgets::BorderType {
    match s.trim().to_ascii_lowercase().as_str() {
        "plain" | "thin" => ratatui::widgets::BorderType::Plain,
        "rounded" => ratatui::widgets::BorderType::Rounded,
        "double" => ratatui::widgets::BorderType::Double,
        _ => ratatui::widgets::BorderType::Thick,
    }
}

pub(super) fn parse_blink_rate(s: &str) -> BlinkRate {
    match s.trim().to_ascii_lowercase().as_str() {
        "slow" => BlinkRate::Slow,
        "rapid" | "fast" => BlinkRate::Rapid,
        _ => BlinkRate::Normal,
    }
}

pub(super) fn apply_nav_props(props: &[String], config: &mut Config) {
    config.render.ui.nav.active = true;
    config.render.ui.nav.bar = None;
    config.render.action.border.sides = Some(ratatui::widgets::Borders::NONE);
    config.render.query.status_inline = true;

    for raw in props {
        for prop in raw.split(',').filter(|s| !s.is_empty()) {
            match prop.split_once(':') {
                None => match prop {
                    "bar" => {
                        config.render.ui.nav.bar = Some(ratatui::widgets::BorderType::Thick);
                    }
                    "action-bar" => {
                        config.render.action.border.sides = Some(ratatui::widgets::Borders::BOTTOM);
                    }
                    "blink" => config.render.ui.nav.blink = true,
                    "bold" => config.render.ui.nav.bold = true,
                    "notify" => config.render.ui.nav.notify = true,
                    "passthrough" => config.render.ui.nav.passthrough = true,
                    "no-filter" => {
                        config.render.query.show = false;
                    }
                    "basic" => {
                        config.render.ui.nav.basic = true;
                        config.render.ui.nav.profile = waymaker::config::NavProfile::Basic;
                    }
                    "list" | "picker" => {
                        config.render.ui.nav.profile = waymaker::config::NavProfile::List;
                    }
                    "fm" | "filemanager" | "jump" => {
                        config.render.ui.nav.profile = waymaker::config::NavProfile::Fm;
                    }
                    "none" | "strict" => {
                        config.render.ui.nav.profile = waymaker::config::NavProfile::None;
                    }
                    "hints" => config.render.ui.nav.hints = true,
                    "no-hints" => config.render.ui.nav.hints = false,
                    "parent-peek" | "parent_peek" => config.render.ui.parent_peek.enabled = true,
                    "no-parent-peek" | "no_parent_peek" => {
                        config.render.ui.parent_peek.enabled = false
                    }
                    "status-inline" => config.render.query.status_inline = true,
                    "no-status-inline" => config.render.query.status_inline = false,
                    _ => eprintln!("warning: unknown --nav property '{}'", prop),
                },
                Some(("profile" | "mode", s)) => match s.trim().to_ascii_lowercase().as_str() {
                    "fm" | "filemanager" | "jump" => {
                        config.render.ui.nav.profile = waymaker::config::NavProfile::Fm;
                    }
                    "basic" | "minimal" => {
                        config.render.ui.nav.basic = true;
                        config.render.ui.nav.profile = waymaker::config::NavProfile::Basic;
                    }
                    "none" | "strict" => {
                        config.render.ui.nav.profile = waymaker::config::NavProfile::None;
                    }
                    _ => config.render.ui.nav.profile = waymaker::config::NavProfile::List,
                },
                Some(("bar", s)) => {
                    config.render.ui.nav.bar = Some(parse_border_type(s));
                }
                Some(("action-bar", s)) => {
                    config.render.action.border.sides = Some(ratatui::widgets::Borders::BOTTOM);
                    config.render.action.border.r#type = Some(parse_border_type(s));
                }
                Some(("blink", s)) => {
                    config.render.ui.nav.blink = true;
                    config.render.ui.nav.blink_rate = parse_blink_rate(s);
                }
                Some(("hints", s)) => match s.trim().to_ascii_lowercase().as_str() {
                    "false" | "off" | "no" | "0" => config.render.ui.nav.hints = false,
                    _ => config.render.ui.nav.hints = true,
                },
                Some(("parent-peek" | "parent_peek", s)) => {
                    match s.trim().to_ascii_lowercase().as_str() {
                        "false" | "off" | "no" | "0" => {
                            config.render.ui.parent_peek.enabled = false
                        }
                        _ => config.render.ui.parent_peek.enabled = true,
                    }
                }
                Some(("parent-peek-pct" | "parent_peek_pct", s)) => {
                    if let Ok(pct) = s.parse::<u16>() {
                        config.render.ui.parent_peek.pct = waymaker::config::Percentage::new(pct);
                    }
                }
                Some(("parent-peek-color" | "parent_peek_color", s)) => {
                    if let Ok(color) = s.trim().parse::<ratatui::style::Color>() {
                        config.render.ui.parent_peek.parent_color = Some(color);
                    }
                }
                Some(("parent-peek-highlight" | "parent_peek_highlight", s)) => {
                    match s.trim().to_ascii_lowercase().as_str() {
                        "false" | "off" | "no" | "0" => {
                            config.render.ui.parent_peek.highlight = false
                        }
                        _ => config.render.ui.parent_peek.highlight = true,
                    }
                }
                Some(("parent-peek-highlight-color" | "parent_peek_highlight_color", s)) => {
                    if let Ok(color) = s.trim().parse::<ratatui::style::Color>() {
                        config.render.ui.parent_peek.highlight_color = Some(color);
                    }
                }
                Some(("parent-peek-border" | "parent_peek_border", s)) => {
                    match s.trim().to_ascii_lowercase().as_str() {
                        "false" | "off" | "no" | "0" | "none" => {
                            config.render.ui.parent_peek.border.show = false
                        }
                        _ => config.render.ui.parent_peek.border.show = true,
                    }
                }
                Some(("parent-peek-border-color" | "parent_peek_border_color", s)) => {
                    if let Ok(color) = s.trim().parse::<ratatui::style::Color>() {
                        config.render.ui.parent_peek.border.color = Some(color);
                    }
                }
                Some(("parent-peek-border-type" | "parent_peek_border_type", s)) => {
                    config.render.ui.parent_peek.border.r#type = Some(parse_border_type(s));
                }
                Some(("focus-on-start", s)) => match s.trim().to_ascii_lowercase().as_str() {
                    "picker" => {
                        config.render.ui.nav.focus_on_start = waymaker::config::NavFocus::Picker
                    }
                    _ => config.render.ui.nav.focus_on_start = waymaker::config::NavFocus::Filter,
                },
                Some(("marker", s)) => config.render.ui.nav.marker = s.to_string(),
                Some(("prompt", s)) => config.render.ui.nav.prompt = s.to_string(),
                Some(("color", s)) => match s.trim().parse::<ratatui::style::Color>() {
                    Ok(color) => config.render.ui.nav.color = color,
                    Err(e) => eprintln!("warning: invalid --nav color '{}': {}", s, e),
                },
                Some((k, _)) => eprintln!("warning: unknown --nav property '{}'", k),
            }
        }
    }
}

pub(super) fn set_media_size(config: &mut Config, size: u32) {
    config.previewer.media_size = size;
    config.render.preview.media.size = Some(size);
}

pub(super) fn apply_media_size_str(s: &str, config: &mut Config) {
    config.render.preview.media.active = true;
    match s.to_ascii_lowercase().as_str() {
        "xs" => set_media_size(config, 128),
        "s" => set_media_size(config, 256),
        "m" => set_media_size(config, 512),
        "l" => set_media_size(config, 1024),
        "xl" => set_media_size(config, 2048),
        "full" | "0" | "none" => set_media_size(config, 0),
        _ => {
            if let Ok(num) = s.parse::<u32>() {
                set_media_size(config, num);
            } else {
                eprintln!("warning: invalid --media-size value '{}'", s);
            }
        }
    }
}

pub(super) fn apply_media_props(props: &[String], config: &mut Config) {
    config.render.preview.media.active = true;

    for raw in props {
        for prop in raw.split(',').filter(|s| !s.is_empty()) {
            match prop.split_once(':') {
                None => {
                    // Try parsing as a standalone protocol or size
                    match prop.to_ascii_lowercase().as_str() {
                        "kitty" | "sixel" | "halfblocks" | "iterm2" => {
                            config.render.preview.media.protocol = Some(prop.to_string());
                        }
                        "xs" => set_media_size(config, 128),
                        "s" => set_media_size(config, 256),
                        "m" => set_media_size(config, 512),
                        "l" => set_media_size(config, 1024),
                        "xl" => set_media_size(config, 2048),
                        "full" | "0" | "none" => set_media_size(config, 0),
                        _ => {
                            if let Ok(num) = prop.parse::<u32>() {
                                set_media_size(config, num);
                            } else {
                                eprintln!("warning: unknown --media property '{}'", prop);
                            }
                        }
                    }
                }
                Some(("size", s)) => match s.to_ascii_lowercase().as_str() {
                    "xs" => set_media_size(config, 128),
                    "s" => set_media_size(config, 256),
                    "m" => set_media_size(config, 512),
                    "l" => set_media_size(config, 1024),
                    "xl" => set_media_size(config, 2048),
                    "full" | "0" | "none" => set_media_size(config, 0),
                    _ => {
                        if let Ok(num) = s.parse::<u32>() {
                            set_media_size(config, num);
                        } else {
                            eprintln!("warning: invalid --media size value '{}'", s);
                        }
                    }
                },
                Some(("type" | "protocol", s)) => {
                    config.render.preview.media.protocol = Some(s.to_string());
                }
                Some((k, _)) => eprintln!("warning: unknown --media property '{}'", k),
            }
        }
    }
}

/// Split a nav-bind action string on ';' while ignoring semicolons inside
/// parentheses. This allows `Execute(cd {};ls)` to stay a single action.
pub(super) fn split_nav_bind_actions(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: usize = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }

    let tail = s[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }

    parts
}
