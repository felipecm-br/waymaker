use super::*;

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutoscrollSettings {
    /// Number of characters at the start of the line to always keep visible.
    #[partial(alias = "i")]
    pub initial_preserved: usize,
    /// Enable/disable horizontal autoscroll.
    #[partial(alias = "a")]
    pub enabled: bool,
    /// Number of characters to show around the match.
    #[partial(alias = "c")]
    pub context: usize,
    /// Whether to autoscroll to the end of the line.
    #[partial(alias = "e")]
    pub end: bool,
    /// Enable autoscroll even when wrap = true. Ignored if enable = false.
    pub always: bool,
    /// The string to show when text is horizontally scrolled.
    pub indicator: String,
    /// Style of the hscroll indicator.
    pub indicator_style: StyleSetting,
}

impl Default for AutoscrollSettings {
    fn default() -> Self {
        Self {
            initial_preserved: 0,
            enabled: true,
            context: 4,
            end: false,
            always: false,
            indicator: "..".to_string(),
            indicator_style: crate::config_types::StyleSetting {
                fg: Some(ratatui::style::Color::DarkGray),
                ..Default::default()
            },
        }
    }
}

/// Symlink target display configuration.
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResultsSymlinkConfig {
    /// Show symlink targets appended to the first column text.
    /// Defaults to false.
    #[serde(alias = "symlink_target", alias = "target")]
    pub active: bool,
    /// Style for the appended symlink target text.
    #[partial(recurse)]
    #[serde(alias = "symlink_target_style")]
    pub style: StyleSetting,
}

impl Default for ResultsSymlinkConfig {
    fn default() -> Self {
        Self {
            active: false,
            style: StyleSetting {
                fg: Some(Color::DarkGray),
                ..Default::default()
            },
        }
    }
}

pub fn deserialize_results_symlink<'de, D>(
    deserializer: D,
) -> Result<ResultsSymlinkConfig, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SymlinkHelper {
        Bool(bool),
        Config(ResultsSymlinkConfig),
    }

    match SymlinkHelper::deserialize(deserializer)? {
        SymlinkHelper::Bool(b) => Ok(ResultsSymlinkConfig {
            active: b,
            ..ResultsSymlinkConfig::default()
        }),
        SymlinkHelper::Config(c) => Ok(c),
    }
}

/// Tier separator line configuration for 3-tiered sort when dir_first is enabled.
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResultsTierConfig {
    /// Horizontal separator style drawn between the 3 tiers (direct dirs, direct files, deep items)
    /// when dir_first is enabled. Defaults to Top ("▔").
    #[serde(deserialize_with = "camelcase_normalized")]
    #[serde(alias = "tier_separator")]
    pub separator: HorizontalSeparator,
    /// Style override for the tier separator line.
    #[partial(recurse)]
    #[serde(alias = "tier_separator_style")]
    pub style: StyleSetting,
}

impl Default for ResultsTierConfig {
    fn default() -> Self {
        Self {
            separator: HorizontalSeparator::Top,
            style: StyleSetting {
                fg: Some(Color::DarkGray),
                ..Default::default()
            },
        }
    }
}

/// Icon configuration for pinned / bookmarked items.
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResultsBookmarkConfig {
    /// Icon for pinned / bookmarked items.
    #[serde(alias = "bookmark_icon")]
    pub icon: Option<String>,
    /// Icon for pinned / bookmarked file items. Defaults to Some("󱀻".to_string()).
    #[serde(alias = "bookmark_file_icon")]
    pub file_icon: Option<String>,
    /// Icon for pinned / bookmarked folder items. Defaults to Some("󰮟".to_string()).
    #[serde(alias = "bookmark_folder_icon")]
    pub folder_icon: Option<String>,
    /// Style / color for the bookmark icon. Defaults to Yellow.
    #[partial(recurse)]
    #[serde(alias = "bookmark_icon_style")]
    pub icon_style: StyleSetting,
    /// Style / color for the bookmark file icon. Defaults to Yellow.
    #[partial(recurse)]
    #[serde(alias = "bookmark_file_icon_style")]
    pub file_icon_style: StyleSetting,
    /// Style / color for the bookmark folder icon. Defaults to Yellow.
    #[partial(recurse)]
    #[serde(alias = "bookmark_folder_icon_style")]
    pub folder_icon_style: StyleSetting,
}

impl Default for ResultsBookmarkConfig {
    fn default() -> Self {
        Self {
            icon: None,
            file_icon: Some("󱀻".to_string()),
            folder_icon: Some("󰮟".to_string()),
            icon_style: StyleSetting {
                fg: Some(Color::Yellow),
                ..Default::default()
            },
            file_icon_style: Default::default(),
            folder_icon_style: Default::default(),
        }
    }
}

/// Icon configuration for frecency-scored items.
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResultsFrecencyConfig {
    /// Icon for frecency folder items. Defaults to Some("󰪻".to_string()).
    #[serde(alias = "frecency_folder_icon")]
    pub folder_icon: Option<String>,
    /// Style / color for the frecency folder icon. Defaults to Blue.
    #[partial(recurse)]
    #[serde(alias = "frecency_folder_icon_style")]
    pub folder_icon_style: StyleSetting,
    /// Icon for frecency general / file items. Defaults to Some("󱋢".to_string()).
    #[serde(alias = "frecency_icon")]
    pub icon: Option<String>,
    /// Style / color for the frecency item icon. Defaults to Blue.
    #[partial(recurse)]
    #[serde(alias = "frecency_icon_style")]
    pub icon_style: StyleSetting,
}

impl Default for ResultsFrecencyConfig {
    fn default() -> Self {
        Self {
            folder_icon: Some("󰪻".to_string()),
            folder_icon_style: StyleSetting {
                fg: Some(Color::Blue),
                ..Default::default()
            },
            icon: Some("󱋢".to_string()),
            icon_style: StyleSetting {
                fg: Some(Color::Blue),
                ..Default::default()
            },
        }
    }
}

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResultsConfig {
    #[partial(recurse)]
    #[partial(alias = "b")]
    pub border: BorderSetting,

    // prefixes
    #[serde(deserialize_with = "deserialize_string_or_char_as_double_width")]
    pub multi_prefix: String,
    pub unselected_prefix: String,
    pub default_prefix: String,

    pub spinner_prefix: String,
    pub spinner: String,
    pub spinner_inline: bool,
    /// A second marker, animated with `slow_spinner`, for rows in a calmer busy state.
    pub slow_spinner_prefix: String,
    pub slow_spinner: String,

    #[serde(alias = "prefix")]
    #[partial(recurse)]
    pub prefix_style: StyleSetting,

    #[serde(alias = "prefix_inactive")]
    #[partial(recurse)]
    pub prefix_inactive_style: StyleSetting,

    #[partial(recurse)]
    pub unselected_prefix_style: StyleSetting,

    #[partial(recurse)]
    pub spinner_style: StyleSetting,

    /// Enable selections
    pub multi: bool,

    // text styles
    #[partial(recurse)]
    pub style: StyleSetting,

    // inactive_col styles
    #[serde(alias = "inactive")]
    #[partial(recurse)]
    pub inactive_style: StyleSetting,

    // inactive_col styles on the current item
    #[serde(alias = "inactive_current")]
    #[partial(recurse)]
    pub inactive_current_style: StyleSetting,

    #[serde(alias = "match")]
    #[partial(recurse)]
    pub match_style: StyleSetting,

    /// current item style
    #[serde(alias = "current")]
    #[partial(recurse)]
    pub current_style: StyleSetting,

    /// How the styles are applied across the row:
    /// Disjoint: Styles are applied per column.
    /// Capped: The inactive styles are applied per row, and the active styles applied on the active column.
    /// Full: Inactive column styles are ignored, the current style is applied on the current row.
    #[serde(deserialize_with = "camelcase_normalized")]
    pub row_connection: RowConnectionStyle,

    // scroll
    #[partial(alias = "c")]
    #[serde(alias = "cycle")]
    pub scroll_wrap: bool,
    #[partial(alias = "sp")]
    pub scroll_padding: u16,
    #[partial(alias = "r")]
    pub reverse: Option<bool>,

    // wrap
    #[partial(alias = "w")]
    pub wrap: bool,
    pub min_width: u16,

    // autoscroll
    #[partial(recurse, alias = "a")]
    pub autoscroll: AutoscrollSettings,

    // ------------
    // experimental
    // ------------
    pub column_spacing: Count,
    pub current_prefix: String,

    /// Maximum row height.
    /// VScroll/Preview can still be used to view the whole result.
    pub max_height: usize,
    #[serde(alias = "max")]
    pub max_rows: Option<u16>,
    pub show_skipped: bool,
    /// Always false if max_height is set
    pub vscroll_current_only: bool,

    // lowpri: maybe space-around/space-between instead?
    #[partial(alias = "ra")]
    pub right_align_last: bool,
    #[partial(alias = "v")]
    #[serde(alias = "vertical")]
    pub stacked_columns: bool,

    #[serde(alias = "hr")]
    #[serde(deserialize_with = "camelcase_normalized")]
    pub separator: HorizontalSeparator,
    pub separator_style: StyleSetting,

    /// Prepend a file-type icon (Nerd Font) before the first column text.
    /// Requires a Nerd Font in the terminal. Defaults to false.
    pub icons: bool,
    /// Whether file icons on the focused/cursor row should lose their individual color
    /// to match the cursor line highlight (similar to Yazi). Defaults to false.
    pub uncolor_current_icon: bool,
    /// Whether file icons on the focused/cursor row should invert their color
    /// (e.g. Blue <-> Yellow, Red <-> Cyan, Green <-> Magenta). Defaults to false.
    pub invert_current_icon: bool,
    /// Optional custom style override for the icon on the focused/cursor row.
    #[partial(recurse)]
    pub current_icon_style: StyleSetting,

    /// Border type / thickness override for the navigation bar on the focused/cursor row
    /// (e.g. "Thick", "Plain", "Double", "Rounded", "QuadrantOutside"). Defaults to None.
    pub current_nav_bar: Option<BorderType>,
    /// Optional custom style override for the navigation bar cell on the focused/cursor row.
    #[partial(recurse)]
    pub current_nav_bar_style: StyleSetting,

    /// Symlink display settings.
    #[partial(recurse)]
    #[serde(deserialize_with = "deserialize_results_symlink", default)]
    pub symlink: ResultsSymlinkConfig,

    /// Style for multi-selected rows (not the cursor row).
    #[partial(recurse)]
    pub selected_style: StyleSetting,
    /// Style for the prefix marker of multi-selected rows.
    #[partial(recurse)]
    pub selected_prefix_style: StyleSetting,
    /// Style for the prefix marker of yanked rows (FM mode).
    #[partial(recurse)]
    pub yank_prefix_style: StyleSetting,
    /// Style for the prefix marker of cut rows (FM mode).
    #[partial(recurse)]
    pub cut_prefix_style: StyleSetting,

    /// Style for group header rows (set via --color group-header:…)
    #[serde(alias = "group_header")]
    #[partial(recurse)]
    pub group_header_style: StyleSetting,

    /// Style for inactive group header rows (when the cursor is not in this group)
    #[serde(alias = "inactive_group_header")]
    #[partial(recurse)]
    pub inactive_group_header_style: StyleSetting,

    /// Tier separator settings.
    #[partial(recurse)]
    pub tier: ResultsTierConfig,

    /// Initial cursor position (0-based index or negative for from-the-end).
    pub pos: Option<i32>,

    /// Bookmark icons and styles.
    #[partial(recurse)]
    pub bookmark: ResultsBookmarkConfig,

    /// Frecency icons and styles.
    #[partial(recurse)]
    pub frecency: ResultsFrecencyConfig,
}

impl Default for ResultsConfig {
    fn default() -> Self {
        ResultsConfig {
            pos: None,
            border: Default::default(),

            multi_prefix: "▌ ".to_string(),
            unselected_prefix: "  ".to_string(),
            default_prefix: Default::default(),
            spinner_prefix: "".to_string(),
            spinner: "dot".to_string(),
            spinner_inline: false,
            slow_spinner_prefix: "".to_string(),
            slow_spinner: "hexfill".to_string(),
            prefix_style: Default::default(),
            prefix_inactive_style: Default::default(),
            unselected_prefix_style: Default::default(),
            spinner_style: Default::default(),
            multi: true,

            style: Default::default(),
            inactive_style: Default::default(),

            inactive_current_style: StyleSetting {
                // fg: Some(Color::DarkGray),
                // bg: Some(Color::Black),
                ..Default::default()
            },

            match_style: StyleSetting {
                fg: Some(Color::Green),
                modifier: Modifier::ITALIC,
                ..Default::default()
            },

            current_style: StyleSetting {
                bg: Some(Color::Reset),
                modifier: Modifier::BOLD,
                ..Default::default()
            },

            row_connection: RowConnectionStyle::Capped,

            scroll_wrap: false,
            scroll_padding: 2,
            reverse: None,

            wrap: false,
            min_width: 2,
            max_height: 0,
            max_rows: None,

            autoscroll: Default::default(),

            column_spacing: Default::default(),
            current_prefix: Default::default(),
            right_align_last: false,
            stacked_columns: false,
            separator: Default::default(),
            separator_style: Default::default(),
            show_skipped: true,
            vscroll_current_only: true,
            icons: false,
            uncolor_current_icon: false,
            invert_current_icon: false,
            current_icon_style: Default::default(),
            current_nav_bar: Some(BorderType::Plain),
            current_nav_bar_style: StyleSetting {
                fg: Some(Color::Cyan),
                ..Default::default()
            },
            symlink: ResultsSymlinkConfig::default(),
            selected_style: StyleSetting {
                modifier: Modifier::BOLD,
                ..Default::default()
            },
            selected_prefix_style: StyleSetting {
                fg: Some(Color::Cyan),
                modifier: Modifier::BOLD,
                ..Default::default()
            },
            yank_prefix_style: StyleSetting {
                fg: Some(Color::Yellow),
                modifier: Modifier::BOLD,
                ..Default::default()
            },
            cut_prefix_style: StyleSetting {
                fg: Some(Color::Red),
                modifier: Modifier::BOLD,
                ..Default::default()
            },
            group_header_style: StyleSetting {
                fg: Some(Color::Cyan),
                modifier: Modifier::BOLD,
                ..Default::default()
            },
            inactive_group_header_style: StyleSetting {
                fg: Some(Color::Cyan),
                modifier: Modifier::DIM,
                ..Default::default()
            },
            tier: ResultsTierConfig::default(),
            bookmark: ResultsBookmarkConfig::default(),
            frecency: ResultsFrecencyConfig::default(),
        }
    }
}

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StatusConfig {
    #[partial(recurse)]
    pub style: StyleSetting,

    /// Whether the status is visible.
    pub show: bool,
    /// Indent the status to match the results.
    pub match_indent: bool,

    /// Supports replacements:
    /// - `\r` -> cursor index
    /// - `\m` -> match count
    /// - `\t` -> total count
    /// - `\s` -> available whitespace / # appearances
    /// - `\S` -> Increment # appearances for `\s`
    ///
    /// For example: `r#"\m/\t"#.to_string()`
    #[partial(alias = "t")]
    pub template: String,

    /// - Full: available whitespace is computed using the full ui width when replacing `\s` in the template.
    /// - Disjoint: no effect.
    /// - Capped: no effect. (Since, unlike [`DisplayConfig`], status line can not display over the preview).
    pub row_connection: RowConnectionStyle,

    pub interactions: InteractionRegionSetting,
}
impl Default for StatusConfig {
    fn default() -> Self {
        Self {
            style: StyleSetting {
                fg: Some(Color::Green),
                modifier: Modifier::ITALIC,
                ..Default::default()
            },
            show: true,
            match_indent: true,
            template: String::new(),
            row_connection: RowConnectionStyle::Full,

            interactions: Default::default(),
        }
    }
}

impl StatusConfig {}

pub fn deserialize_horizontal_separator<'de, D>(
    deserializer: D,
) -> Result<HorizontalSeparator, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SeparatorHelper {
        Bool(bool),
        String(String),
    }

    match SeparatorHelper::deserialize(deserializer)? {
        SeparatorHelper::Bool(b) => Ok(if b {
            HorizontalSeparator::Light
        } else {
            HorizontalSeparator::None
        }),
        SeparatorHelper::String(s) => {
            match s.to_lowercase().replace(['-', '_', ' '], "").as_str() {
                "none" | "false" | "" => Ok(HorizontalSeparator::None),
                "empty" => Ok(HorizontalSeparator::Empty),
                "light" | "normal" | "true" => Ok(HorizontalSeparator::Light),
                "heavy" | "thick" | "bold" => Ok(HorizontalSeparator::Heavy),
                "dashed" => Ok(HorizontalSeparator::Dashed),
                "top" | "upper" | "upperblock" => Ok(HorizontalSeparator::Top),
                "bottom" | "lower" | "lowerblock" => Ok(HorizontalSeparator::Bottom),
                "underline" | "underlined" => Ok(HorizontalSeparator::Underline),
                other => Err(serde::de::Error::unknown_variant(
                    other,
                    &[
                        "none",
                        "light",
                        "normal",
                        "heavy",
                        "dashed",
                        "top",
                        "bottom",
                        "underline",
                        "true",
                        "false",
                    ],
                )),
            }
        }
    }
}
