use super::*;
use std::path::Path;

#[test]
fn test_preview_config_trim_commands() {
    let mut config = PreviewConfig {
        layout: vec![
            PreviewSetting {
                command: "  echo hello  ".to_string(),
                ..Default::default()
            },
            PreviewSetting {
                command: "\nls -la\n".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    config.trim_commands();

    assert_eq!(config.layout[0].command, "echo hello");
    assert_eq!(config.layout[1].command, "ls -la");
}

#[test]
fn test_preview_config_diagram_options_toml() {
    let default_cfg: PreviewConfig = toml::from_str("").unwrap();
    assert_eq!(default_cfg.diagrams.theme, DiagramTheme::Auto);
    assert_eq!(
        default_cfg.diagrams.background,
        DiagramBackground::Transparent
    );

    let toml_str = r#"
        [diagrams]
        theme = "dark"
        background = "solid"
    "#;
    let config: PreviewConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.diagrams.theme, DiagramTheme::Dark);
    assert_eq!(config.diagrams.background, DiagramBackground::Solid);

    // Test aliases
    let toml_alias = r#"
        [diagrams]
        diagrams_theme = "light"
        diagram_bg = "transparent"
    "#;
    let config_alias: PreviewConfig = toml::from_str(toml_alias).unwrap();
    assert_eq!(config_alias.diagrams.theme, DiagramTheme::Light);
    assert_eq!(
        config_alias.diagrams.background,
        DiagramBackground::Transparent
    );
}

#[test]
fn test_results_config_tier_separator_toml() {
    let toml_str = r#"
        [tier]
        separator = "dashed"
        [tier.style]
        fg = "Cyan"
    "#;
    let config: ResultsConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.tier.separator, HorizontalSeparator::Dashed);
    assert_eq!(config.tier.style.fg, Some(ratatui::style::Color::Cyan));

    let toml_top = r#"
        [tier]
        separator = "top"
    "#;
    let config_top: ResultsConfig = toml::from_str(toml_top).unwrap();
    assert_eq!(config_top.tier.separator, HorizontalSeparator::Top);

    let toml_bottom = r#"
        [tier]
        separator = "bottom"
    "#;
    let config_bottom: ResultsConfig = toml::from_str(toml_bottom).unwrap();
    assert_eq!(config_bottom.tier.separator, HorizontalSeparator::Bottom);
}

#[test]
fn test_folder_rule_matching() {
    let rule_downloads = FolderRule::new("~/Downloads", SortOrder::ModifiedReverse);
    if let Some(home) = dirs::home_dir() {
        let downloads = home.join("Downloads");
        assert!(rule_downloads.matches(&downloads));
        let other = home.join("Documents");
        assert!(!rule_downloads.matches(&other));
    }

    let rule_glob = FolderRule::new("*/screenshots", SortOrder::CreatedReverse);
    assert!(rule_glob.matches(Path::new("/home/user/pictures/screenshots")));
    assert!(rule_glob.matches(Path::new("/var/data/screenshots")));
    assert!(!rule_glob.matches(Path::new("/home/user/pictures/wallpapers")));

    let rule_basename = FolderRule::new("Screenshots", SortOrder::CreatedReverse);
    assert!(rule_basename.matches(Path::new("/home/user/Pictures/Screenshots")));
}

#[test]
fn test_ui_config_resolve_sort_for_dir() {
    let mut ui = UiConfig::default();
    ui.default_sort = Some(SortOrder::Natural);
    ui.folder_rules = vec![
        FolderRule::new("~/Downloads", SortOrder::ModifiedReverse),
        FolderRule::new("*/videos", SortOrder::SizeReverse),
    ];

    if let Some(home) = dirs::home_dir() {
        assert_eq!(
            ui.resolve_sort_for_dir(&home.join("Downloads")),
            Some(SortOrder::ModifiedReverse)
        );
        assert_eq!(
            ui.resolve_sort_for_dir(&home.join("Projects")),
            Some(SortOrder::Natural)
        );
    }
    assert_eq!(
        ui.resolve_sort_for_dir(Path::new("/media/storage/videos")),
        Some(SortOrder::SizeReverse)
    );
}

#[test]
fn test_nav_profile_deserialization_and_effective_profile() {
    // Default
    let default_nav: NavConfig = toml::from_str("").unwrap();
    assert_eq!(default_nav.profile, NavProfile::List);
    assert_eq!(default_nav.effective_profile(), NavProfile::List);

    // Explicit list
    let nav_list: NavConfig = toml::from_str(r#"profile = "list""#).unwrap();
    assert_eq!(nav_list.effective_profile(), NavProfile::List);

    // Alias picker
    let nav_picker: NavConfig = toml::from_str(r#"profile = "picker""#).unwrap();
    assert_eq!(nav_picker.effective_profile(), NavProfile::List);

    // Basic via profile
    let nav_basic: NavConfig = toml::from_str(r#"profile = "basic""#).unwrap();
    assert_eq!(nav_basic.effective_profile(), NavProfile::Basic);

    // Basic via legacy flag
    let nav_legacy_basic: NavConfig = toml::from_str(r#"basic = true"#).unwrap();
    assert_eq!(nav_legacy_basic.effective_profile(), NavProfile::Basic);

    // Fm profile
    let nav_fm: NavConfig = toml::from_str(r#"profile = "fm""#).unwrap();
    assert_eq!(nav_fm.effective_profile(), NavProfile::Fm);

    // Fm via fm flag
    let nav_fm_flag: NavConfig = toml::from_str(r#"fm = true"#).unwrap();
    assert_eq!(nav_fm_flag.effective_profile(), NavProfile::Fm);

    // None / strict profile
    let nav_none: NavConfig = toml::from_str(r#"profile = "none""#).unwrap();
    assert_eq!(nav_none.effective_profile(), NavProfile::None);

    let nav_strict: NavConfig = toml::from_str(r#"profile = "strict""#).unwrap();
    assert_eq!(nav_strict.effective_profile(), NavProfile::None);
}

#[test]
fn test_default_binds_for_profile_isolation() {
    let list_binds = NavConfig::default_binds_for_profile(NavProfile::List);
    assert!(list_binds.contains_key("j"));
    assert!(list_binds.contains_key("k"));
    assert!(list_binds.contains_key(" "));
    assert!(list_binds.contains_key("/"));
    // Must NOT contain file-manager binds
    assert!(!list_binds.contains_key("h"));
    assert!(!list_binds.contains_key("l"));
    assert!(!list_binds.contains_key("f"));
    assert!(!list_binds.contains_key("b"));
    assert!(!list_binds.contains_key("*"));

    let basic_binds = NavConfig::default_binds_for_profile(NavProfile::Basic);
    assert!(basic_binds.contains_key("j"));
    assert!(basic_binds.contains_key("k"));
    assert!(basic_binds.contains_key("/"));
    // Basic does not have extra navigation
    assert!(!basic_binds.contains_key("gb"));
    assert!(!basic_binds.contains_key("gt"));
    assert!(!basic_binds.contains_key("h"));
    assert!(!basic_binds.contains_key("l"));

    let fm_binds = NavConfig::default_binds_for_profile(NavProfile::Fm);
    assert!(fm_binds.contains_key("j"));
    assert!(fm_binds.contains_key("k"));
    assert!(fm_binds.contains_key("h"));
    assert!(fm_binds.contains_key("l"));
    assert!(fm_binds.contains_key("f"));
    assert!(fm_binds.contains_key("b"));
    assert!(fm_binds.contains_key("*"));

    let none_binds = NavConfig::default_binds_for_profile(NavProfile::None);
    assert!(none_binds.is_empty());
}

#[test]
fn test_display_config_separator_deserialization() {
    // String variant
    let toml_str = r#"
        separator = "light"
    "#;
    let cfg: DisplayConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.separator, HorizontalSeparator::Light);

    // Boolean variant (true)
    let toml_bool: DisplayConfig = toml::from_str("separator = true").unwrap();
    assert_eq!(toml_bool.separator, HorizontalSeparator::Light);

    // Boolean variant (false)
    let toml_false: DisplayConfig = toml::from_str("separator = false").unwrap();
    assert_eq!(toml_false.separator, HorizontalSeparator::None);

    // Alias hr
    let toml_hr: DisplayConfig = toml::from_str(r#"hr = "heavy""#).unwrap();
    assert_eq!(toml_hr.separator, HorizontalSeparator::Heavy);

    // Default
    let toml_def: DisplayConfig = toml::from_str("").unwrap();
    assert_eq!(toml_def.separator, HorizontalSeparator::None);
}
