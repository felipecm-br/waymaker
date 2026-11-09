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

/// The pixel limit a media size names: `xs`, `s`, `m`, `l`, `xl`, `full`/`0`/`none` (no limit),
/// or a number.
pub(super) fn parse_media_size(s: &str) -> Option<u32> {
    match s.to_ascii_lowercase().as_str() {
        "xs" => Some(128),
        "s" => Some(256),
        "m" => Some(512),
        "l" => Some(1024),
        "xl" => Some(2048),
        "full" | "none" => Some(0),
        _ => s.parse::<u32>().ok(),
    }
}

pub(super) fn apply_media_size_str(s: &str, config: &mut Config) {
    config.render.preview.media.active = true;
    match parse_media_size(s) {
        Some(size) => set_media_size(config, size),
        None => eprintln!("warning: invalid --media-size value '{}'", s),
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
                        _ => match parse_media_size(prop) {
                            Some(size) => set_media_size(config, size),
                            None => eprintln!("warning: unknown --media property '{}'", prop),
                        },
                    }
                }
                Some(("size", s)) => match parse_media_size(s) {
                    Some(size) => set_media_size(config, size),
                    None => eprintln!("warning: invalid --media size value '{}'", s),
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

#[cfg(test)]
mod tests {
    use super::*;
    use waymaker::config::NavProfile;

    fn nav(props: &[&str]) -> Config {
        let mut config = Config::default();
        let props: Vec<String> = props.iter().map(|s| s.to_string()).collect();
        apply_nav_props(&props, &mut config);
        config
    }

    #[test]
    fn nav_turns_navigation_on_with_inline_status_and_no_bar() {
        let c = nav(&[]);
        let nav = &c.render.ui.nav;
        assert!(nav.active);
        assert_eq!(nav.bar, None);
        assert!(c.render.query.status_inline);
        assert_eq!(
            c.render.action.border.sides,
            Some(ratatui::widgets::Borders::NONE)
        );
    }

    #[test]
    fn nav_flags_and_profiles() {
        let c = nav(&["bar,blink,bold,notify", "no-filter,hints,parent-peek"]);
        let n = &c.render.ui.nav;
        assert_eq!(n.bar, Some(ratatui::widgets::BorderType::Thick));
        assert!(n.blink && n.bold && n.notify && n.hints);
        assert!(!c.render.query.show);
        assert!(c.render.ui.parent_peek.enabled);

        assert_eq!(nav(&["fm"]).render.ui.nav.profile, NavProfile::Fm);
        assert_eq!(nav(&["jump"]).render.ui.nav.profile, NavProfile::Fm);
        assert_eq!(nav(&["picker"]).render.ui.nav.profile, NavProfile::List);
        assert_eq!(nav(&["strict"]).render.ui.nav.profile, NavProfile::None);
        let basic = nav(&["profile:minimal"]);
        assert_eq!(basic.render.ui.nav.profile, NavProfile::Basic);
        assert!(basic.render.ui.nav.basic);
        // An unknown profile value falls back to the list profile.
        assert_eq!(
            nav(&["mode:whatever"]).render.ui.nav.profile,
            NavProfile::List
        );
    }

    #[test]
    fn nav_valued_properties() {
        let c = nav(&[
            "bar:double,action-bar:rounded,blink:fast,marker:>,prompt:nav> ,color:#a6e3a1",
            "hints:off,parent-peek-pct:30,focus-on-start:picker",
        ]);
        let n = &c.render.ui.nav;
        assert_eq!(n.bar, Some(ratatui::widgets::BorderType::Double));
        assert_eq!(
            c.render.action.border.sides,
            Some(ratatui::widgets::Borders::BOTTOM)
        );
        assert_eq!(
            c.render.action.border.r#type,
            Some(ratatui::widgets::BorderType::Rounded)
        );
        assert!(n.blink);
        assert_eq!(n.blink_rate, BlinkRate::Rapid);
        assert_eq!(n.marker, ">");
        assert_eq!(n.prompt, "nav> ");
        assert_eq!(n.color, ratatui::style::Color::Rgb(0xa6, 0xe3, 0xa1));
        assert!(!n.hints);
        assert_eq!(
            c.render.ui.parent_peek.pct,
            waymaker::config::Percentage::new(30)
        );
        assert_eq!(n.focus_on_start, waymaker::config::NavFocus::Picker);
    }

    #[test]
    fn nav_ignores_an_invalid_colour_and_unknown_properties() {
        let before = Config::default().render.ui.nav.color;
        let c = nav(&["color:notacolour,frobnicate,what:ever"]);
        assert_eq!(c.render.ui.nav.color, before);
        assert!(c.render.ui.nav.active);
    }

    #[test]
    fn border_type_and_blink_rate_names() {
        use ratatui::widgets::BorderType;
        assert_eq!(parse_border_type(" Thin "), BorderType::Plain);
        assert_eq!(parse_border_type("rounded"), BorderType::Rounded);
        assert_eq!(parse_border_type("double"), BorderType::Double);
        assert_eq!(parse_border_type("anything"), BorderType::Thick);
        assert_eq!(parse_blink_rate("slow"), BlinkRate::Slow);
        assert_eq!(parse_blink_rate("FAST"), BlinkRate::Rapid);
        assert_eq!(parse_blink_rate(""), BlinkRate::Normal);
    }

    #[test]
    fn media_sizes_by_name_or_number() {
        assert_eq!(parse_media_size("xs"), Some(128));
        assert_eq!(parse_media_size("XL"), Some(2048));
        assert_eq!(parse_media_size("full"), Some(0));
        assert_eq!(parse_media_size("none"), Some(0));
        assert_eq!(parse_media_size("0"), Some(0));
        assert_eq!(parse_media_size("640"), Some(640));
        assert_eq!(parse_media_size("big"), None);
    }

    #[test]
    fn media_props_set_protocol_and_size() {
        let mut c = Config::default();
        apply_media_props(&["kitty,s".to_string()], &mut c);
        assert!(c.render.preview.media.active);
        assert_eq!(c.render.preview.media.protocol.as_deref(), Some("kitty"));
        assert_eq!(c.previewer.media_size, 256);

        apply_media_props(&["type:sixel,size:full".to_string()], &mut c);
        assert_eq!(c.render.preview.media.protocol.as_deref(), Some("sixel"));
        assert_eq!(c.render.preview.media.size, Some(0));

        // An unknown size leaves the previous one.
        apply_media_props(&["size:huge,1024".to_string()], &mut c);
        assert_eq!(c.previewer.media_size, 1024);
        apply_media_props(&["size:huge".to_string()], &mut c);
        assert_eq!(c.previewer.media_size, 1024);
    }

    #[test]
    fn nav_bind_actions_split_on_semicolons_outside_parentheses() {
        assert_eq!(
            split_nav_bind_actions("ChDir(..); Reload"),
            vec!["ChDir(..)", "Reload"]
        );
        assert_eq!(
            split_nav_bind_actions("Execute(cd {};ls);Accept"),
            vec!["Execute(cd {};ls)", "Accept"]
        );
        assert_eq!(split_nav_bind_actions("Up;"), vec!["Up"]);
        assert_eq!(split_nav_bind_actions(""), Vec::<&str>::new());
    }
}
