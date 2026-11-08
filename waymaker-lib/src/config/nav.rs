use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NavProfile {
    /// Safe list navigation (cursor movements, selection, paging, filter toggle). No file mutations, no directory hopping.
    #[default]
    #[serde(alias = "picker", alias = "safe")]
    List,
    /// Minimal vertical navigation (j/k, gg/G, J/K, esc, q). Directory hopping silenced.
    #[serde(alias = "minimal")]
    Basic,
    /// Full file-manager navigation (directory hopping with h/l, frecency, bookmarks, and file creation/renaming/deletion actions).
    #[serde(alias = "filemanager", alias = "file_manager", alias = "jump")]
    Fm,
    /// No default bindings injected; strictly uses whatever is defined in `[ui.nav.binds]`.
    #[serde(alias = "strict", alias = "empty")]
    None,
}

///// Configuration for Navigation mode (`--nav`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct NavConfig {
    /// Enable navigation mode: pressing `ToggleFocus` switches keyboard focus
    /// between the input bar and the results list.
    #[partial(alias = "fm")]
    #[serde(alias = "nav_mode", alias = "focus_mode", alias = "mode")]
    pub active: bool,

    /// Navigation profile: "list" (default safe picker), "basic" (minimal vertical),
    /// "fm" (file manager with create/rename/etc.), or "none" (strict zero-defaults).
    #[serde(alias = "nav_profile")]
    #[serde(default)]
    pub profile: NavProfile,

    /// Opt-in flag to enable file manager actions and directory traversal.
    /// Equivalent to setting `profile = "fm"`.
    #[serde(alias = "nav_fm")]
    pub fm: bool,

    /// Navigation-mode indicator colour (set via `--color nav:` or `--nav color:`).
    #[serde(deserialize_with = "camelcase_normalized")]
    #[serde(alias = "nav_color", alias = "focus_color")]
    pub color: Color,

    /// Make the navigation indicator blink.
    #[partial(alias = "fm_blink")]
    #[serde(alias = "nav_blink", alias = "focus_blink")]
    pub blink: bool,

    /// Blink speed when `nav_blink = true`.
    #[partial(alias = "fm_blink_rate")]
    #[serde(alias = "nav_blink_rate", alias = "focus_blink_rate")]
    pub blink_rate: BlinkRate,

    /// Apply bold styling to navigation indicator.
    #[partial(alias = "fm_bold")]
    #[serde(alias = "nav_bold", alias = "focus_bold")]
    pub bold: bool,

    /// Left-bar border style for the navigation indicator.
    #[partial(alias = "fm_bar")]
    #[serde(alias = "nav_bar", alias = "focus_bar")]
    pub bar: Option<BorderType>,

    /// Marker rendered on the current result row when results pane is focused.
    #[partial(alias = "fm_marker")]
    #[serde(alias = "nav_marker", alias = "focus_marker")]
    pub marker: String,

    /// Prompt text shown while the results pane is focused.
    #[partial(alias = "fm_prompt")]
    #[serde(alias = "nav_prompt", alias = "focus_prompt")]
    pub prompt: String,

    /// Optional prompt text shown during interactive prompt input mode (`Action::PromptInput`).
    #[serde(alias = "prompt_input", alias = "nav_prompt_input")]
    pub prompt_input: Option<String>,

    /// Optional indicator text shown during live passthrough input mode (`Action::LiveInput`).
    #[serde(alias = "live_input", alias = "nav_live_input")]
    pub live_input: Option<String>,

    /// Key bindings active while navigation mode is enabled and results pane has focus.
    #[serde(alias = "nav_binds", alias = "focus_binds")]
    #[partial(no_recurse, unwrap)]
    pub binds: HashMap<String, Actions<NullActionExt>>,

    /// Keys of a fullscreen or diagram preview, over the defaults in
    /// [`NavConfig::default_preview_binds`]: a key listed here replaces its default.
    #[partial(no_recurse, unwrap)]
    pub preview_binds: HashMap<String, Actions<NullActionExt>>,

    /// Show notifications for file-manager clipboard actions.
    #[serde(alias = "nav_notify", alias = "focus_notify", alias = "fm_notify")]
    pub notify: bool,

    /// When true, characters typed while the results pane is focused bypass the
    /// nav-bind intercept and fall through to the query input.  Useful when you
    /// want the navigation-mode visual indicators but still want live filtering
    /// while scrolling results.  Enable with `--nav no-filter` or in TOML as
    /// `nav_passthrough = true`.
    #[serde(alias = "nav_passthrough")]
    pub passthrough: bool,

    /// When true only the basic j / k / J / K navigation binds are active;
    /// the h / l / gg / G / gb / gt binds are silenced and the file-manager
    /// semantic binds (d, a, r, …) are not injected.  Enable with
    /// `--nav basic` or in TOML as `nav_basic = true`.
    #[serde(alias = "nav_basic")]
    pub basic: bool,

    #[partial(alias = "fm_focus_on_start")]
    #[serde(alias = "nav_focus_on_start", alias = "focus_on_start")]
    #[serde(default)]
    pub focus_on_start: NavFocus,

    /// Show keybinding hints in footer/status when Results pane is focused in navigation mode.
    #[serde(alias = "nav_hints", alias = "focus_hints", alias = "fm_hints")]
    pub hints: bool,

    /// Number of columns to display in the navigation hints grid in the footer.
    /// If 0 or 1, renders as a single row.
    /// Default: 4.
    #[serde(alias = "nav_hints_columns", alias = "hints_columns")]
    pub hints_columns: usize,
}

impl Default for NavConfig {
    fn default() -> Self {
        Self {
            active: false,
            color: Color::Yellow,
            blink: false,
            blink_rate: BlinkRate::Normal,
            bold: false,
            bar: None,
            marker: "".to_string(),
            prompt: "".to_string(),
            prompt_input: None,
            live_input: None,
            binds: Self::default_binds_for_profile(NavProfile::List),
            preview_binds: HashMap::new(),
            notify: false,
            passthrough: false,
            basic: false,
            profile: NavProfile::List,
            fm: false,
            focus_on_start: NavFocus::Filter,
            hints: true,
            hints_columns: 4,
        }
    }
}

impl NavConfig {
    /// Resolves the effective navigation profile considering `profile`, `fm`, and `basic`.
    pub fn effective_profile(&self) -> NavProfile {
        if self.fm {
            NavProfile::Fm
        } else if self.basic && self.profile == NavProfile::List {
            NavProfile::Basic
        } else {
            self.profile
        }
    }

    /// Returns the default keybindings corresponding to the specified navigation profile.
    /// Adds a default bind unless `key` is already bound, under this or another spelling of the
    /// same key (`Esc`/`esc`, `space`/`" "`), so the user's bind keeps precedence.
    pub fn add_default_bind(&mut self, key: &str, actions: Actions<NullActionExt>) {
        if !self
            .binds
            .keys()
            .any(|k| crate::render::same_nav_key(k, key))
        {
            self.binds.insert(key.to_string(), actions);
        }
    }

    /// The preview keys in effect: `[nav.preview_binds]`, plus each default key it does not bind
    /// under any spelling.
    pub fn effective_preview_binds(&self) -> HashMap<String, Actions<NullActionExt>> {
        Self::layer(&self.preview_binds, Self::default_preview_binds())
    }

    /// The navigation keys in effect: `[nav.binds]` (with the profile's defaults), plus each
    /// [`fallback_binds`](Self::fallback_binds) key it does not bind under any spelling.
    pub fn effective_binds(&self) -> HashMap<String, Actions<NullActionExt>> {
        Self::layer(&self.binds, Self::fallback_binds())
    }

    /// `binds`, plus each of `defaults` whose key `binds` does not bind under any spelling.
    fn layer(
        binds: &HashMap<String, Actions<NullActionExt>>,
        defaults: HashMap<String, Actions<NullActionExt>>,
    ) -> HashMap<String, Actions<NullActionExt>> {
        let mut layered = NavConfig {
            binds: binds.clone(),
            ..Default::default()
        };
        for (key, actions) in defaults {
            layered.add_default_bind(&key, actions);
        }
        layered.binds
    }

    /// Keys every navigation profile has, `none` included, unless `[nav.binds]` binds them:
    /// preview scrolling and diagram controls (while the preview is not fullscreen, where the
    /// preview binds take these keys), the sort menu and the `g` sequences.
    pub fn fallback_binds() -> HashMap<String, Actions<NullActionExt>> {
        let table: [(&[&str], Action<NullActionExt>); 14] = [
            (&["J"], Action::PreviewDown(5)),
            (&["K"], Action::PreviewUp(5)),
            (&["u"], Action::PreviewHalfPageUp),
            (&["d"], Action::PreviewHalfPageDown),
            (&["s"], Action::ToggleDiagram),
            (&["+", "="], Action::DiagramZoomIn),
            (&["-"], Action::DiagramZoomOut),
            (&["0"], Action::DiagramResetZoom),
            (&["]", "n"], Action::NextDiagram),
            (&["[", "N"], Action::PrevDiagram),
            (&[","], Action::SortMenu),
            (&["gg"], Action::PreviewUp(0)),
            (&["gt"], Action::Pos(0)),
            (&["gb"], Action::Pos(-1)),
        ];
        let mut binds = HashMap::new();
        for (keys, action) in table {
            for key in keys {
                binds.insert(key.to_string(), Actions::from([action.clone()]));
            }
        }
        binds
    }

    /// Bound key sequences that can never fire: one whose first key is also bound on its own
    /// (the single key wins), and one longer than two keys. Unbinds (`[]`) do not count.
    pub fn sequence_conflicts(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (key, actions) in &self.binds {
            if actions.0.is_empty() {
                continue;
            }
            let Some(crate::render::NavKey::Sequence(seq)) = crate::render::parse_nav_key(key)
            else {
                continue;
            };
            if seq.len() > 2 {
                problems.push(format!(
                    "'{key}' is a sequence of {} keys; only two-key sequences are supported",
                    seq.len()
                ));
                continue;
            }
            let first: String = key.chars().take(1).collect();
            if let Some((single, _)) = self
                .binds
                .iter()
                .find(|(k, a)| !a.0.is_empty() && crate::render::same_nav_key(k, &first))
            {
                problems.push(format!(
                    "'{key}' starts with '{single}', which is bound on its own, so '{key}' never fires"
                ));
            }
        }
        problems.sort();
        problems
    }

    /// The default keys of a fullscreen or diagram preview; `[nav.preview_binds]` overrides
    /// them key by key.
    pub fn default_preview_binds() -> HashMap<String, Actions<NullActionExt>> {
        let table: [(&[&str], Action<NullActionExt>); 20] = [
            (&["j", "down"], Action::PreviewDown(3)),
            (&["k", "up"], Action::PreviewUp(3)),
            (&["h", "left"], Action::PreviewHScroll(-6)),
            (&["l", "right"], Action::PreviewHScroll(6)),
            (&["J", "shift-down"], Action::PreviewDown(15)),
            (&["K", "shift-up"], Action::PreviewUp(15)),
            (&["H", "shift-left"], Action::PreviewHScroll(-18)),
            (&["L", "shift-right"], Action::PreviewHScroll(18)),
            (&["ctrl-u", "u"], Action::PreviewHalfPageUp),
            (&["ctrl-d", "d"], Action::PreviewHalfPageDown),
            (&["s"], Action::ToggleDiagram),
            (&["n", "]"], Action::NextDiagram),
            (&["N", "["], Action::PrevDiagram),
            (&["+", "="], Action::DiagramZoomIn),
            (&["-"], Action::DiagramZoomOut),
            (&["0"], Action::DiagramResetZoom),
            (&["y"], Action::Accept),
            (&["ctrl-c"], Action::Abort(130)),
            (&["gg"], Action::PreviewUp(0)),
            (&["G"], Action::PreviewDown(0)),
        ];
        let mut binds = HashMap::new();
        for (keys, action) in table {
            for key in keys {
                binds.insert(key.to_string(), Actions::from([action.clone()]));
            }
        }
        binds
    }

    pub fn default_binds_for_profile(
        profile: NavProfile,
    ) -> HashMap<String, Actions<NullActionExt>> {
        let mut binds = HashMap::new();
        if profile == NavProfile::None {
            return binds;
        }

        // Shared navigation binds (Basic, List, Fm)
        binds.insert("j".to_string(), Actions::from([Action::Down(1)]));
        binds.insert("k".to_string(), Actions::from([Action::Up(1)]));
        binds.insert("J".to_string(), Actions::from([Action::PreviewDown(1)]));
        binds.insert("K".to_string(), Actions::from([Action::PreviewUp(1)]));
        binds.insert("gg".to_string(), Actions::from([Action::PreviewUp(0)]));
        binds.insert("G".to_string(), Actions::from([Action::PreviewDown(0)]));
        binds.insert("/".to_string(), Actions::from([Action::FocusFilter]));
        binds.insert("esc".to_string(), Actions::from([Action::Quit(130)]));
        binds.insert("q".to_string(), Actions::from([Action::Quit(130)]));

        if profile == NavProfile::Basic {
            return binds;
        }

        // List and Fm navigation binds
        binds.insert("gb".to_string(), Actions::from([Action::Pos(-1)]));
        binds.insert("gt".to_string(), Actions::from([Action::Pos(0)]));
        binds.insert(" ".to_string(), Actions::from([Action::Toggle]));
        binds.insert("\\".to_string(), Actions::from([Action::ToggleParentPeek]));
        binds.insert("|".to_string(), Actions::from([Action::ToggleParentPeek]));
        binds.insert(",".to_string(), Actions::from([Action::SortMenu]));
        binds.insert(".".to_string(), Actions::from([Action::NextColumn]));
        binds.insert(">".to_string(), Actions::from([Action::PrevColumn]));

        if profile == NavProfile::Fm {
            binds.insert(
                "l".to_string(),
                Actions::from([
                    Action::ChDir("{=}".to_string()),
                    Action::Reload("".to_string()),
                    Action::Pos(0),
                ]),
            );
            binds.insert(
                "h".to_string(),
                Actions::from([
                    Action::ChDir("..".to_string()),
                    Action::Reload("".to_string()),
                    Action::Pos(0),
                ]),
            );
            binds.insert(
                "f".to_string(),
                Actions::from([Action::Semantic("frecency".to_string())]),
            );
            binds.insert(
                "b".to_string(),
                Actions::from([Action::Semantic("bookmarks".to_string())]),
            );
            binds.insert(
                "*".to_string(),
                Actions::from([Action::Semantic("bookmark".to_string())]),
            );
        }

        binds
    }
    /// Calculate the required height in rows for the navigation hints footer.
    pub fn hints_height(&self, item_count: usize) -> u16 {
        if !self.hints {
            return 0;
        }
        let cols = if self.hints_columns == 0 {
            4
        } else {
            self.hints_columns
        };
        if cols <= 1 {
            1
        } else {
            let rows = item_count.div_ceil(cols);
            rows.max(1) as u16
        }
    }
}

pub fn deserialize_nav<'de, D>(deserializer: D) -> Result<NavConfig, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NavHelper {
        Bool(bool),
        Config(Box<NavConfig>),
    }

    match NavHelper::deserialize(deserializer)? {
        NavHelper::Bool(b) => Ok(NavConfig {
            active: b,
            ..NavConfig::default()
        }),
        NavHelper::Config(c) => Ok(*c),
    }
}
