use std::{
    collections::HashMap,
    env::set_current_dir,
    io::{IsTerminal, Read},
    path::Path,
    process::{Command, Stdio, exit},
    sync::{Arc, Mutex},
};

use crate::{
    action::{ActionContext, MMAction, action_handler},
    clap::Cli,
    color::apply_color_spec,
    config::{PartialConfig, parse_warn},
    paths::{last_key_path, presets_path},
    register::MMExt,
    utils::{expand_tilde, guess_clip_cmd, guess_editor_cmd, guess_pager_cmd},
};
use crate::{config::Config, paths::default_config_path};
use cba::{
    _wbog,
    bait::{OptionExt, ResultExt, TransformExt},
    bo::{MapReaderError, write_str},
    bog::BogOkExt,
    ebog, ibog, prints, wbog,
};
use cba::{bo::load_type, broc::CommandExt};
use log::debug;
use waymaker::{
    Action, ConfigInjector, MatchError, Matchmaker, OddEnds, PickOptions, SSS, acs,
    binds::{BindMap, BindMapExt},
    config::{BlinkRate, CommandSetting, EnvValue, MatcherConfig, StartCommandConfig, StartConfig},
    event::{EventLoop, RenderSender},
    make_previewer,
    message::{Event, Interrupt},
    nucleo::{
        ColumnIndexable,
        injector::{AnsiInjector, Either, IndexedInjector, Injector, SegmentedInjector},
    },
    preview::AppendOnly,
    render::MMState,
    use_formatter,
};
use waymaker_partial::Apply;

mod presets;
use presets::*;
mod props;
use props::*;
mod ingest;
use ingest::*;
mod filter;
pub use filter::*;
mod benchmark;
pub use benchmark::*;
mod handlers;
use handlers::*;

pub fn enter(cli: Cli, partial: PartialConfig) -> anyhow::Result<Config> {
    if cli.test_keys {
        super::crokey::main();
        exit(0);
    }

    let cfg_path = if let Some(p) = &cli.config {
        Path::new(p)
    } else {
        default_config_path()
    };

    if cli.dump_config && std::io::stdout().is_terminal() {
        // if stdout: dump the default cfg with comments
        write_str(cfg_path, crate::config::DEFAULT_CONFIG)?;
        ibog!("Config written to {cfg_path:?}");
        exit(0)
    }

    if crate::paths::dev_mode() && cli.config.is_none() {
        #[cfg(target_os = "windows")]
        write_str(cfg_path, include_str!("../../assets/win.dev.toml")).unwrap();

        #[cfg(not(target_os = "windows"))]
        write_str(cfg_path, include_str!("../../assets/dev.toml")).unwrap();
    }

    let mut config: Config = if cli.config.is_some() {
        // Explicit --config: load file verbatim, no default layering.
        load_type(cfg_path, |s| parse_warn(s, cfg_path))
            ._ebog()
            .or_exit()
    } else if cfg_path.exists() {
        // Default user config path: overlay the file on top of the embedded
        // defaults so the user only needs to specify what they want to change
        // (including individual [binds] entries).
        let mut base = Config::default();
        let user: PartialConfig = load_type(cfg_path, |s| parse_warn(s, cfg_path))
            ._ebog()
            .or_exit();
        base.apply(user);
        base
    } else {
        Config::default()
    };
    // check config
    if config.source.is_some() {
        wbog!("'source' field is not supported in the main config.");
    }

    if config.render.status.template.is_empty() {
        config.render.status.template = r#"\m/\t"#.to_string();
    }

    // apply overrides
    for mut p in cli.r#override {
        let mut embedded = None;
        if p.is_relative() && p.extension().is_none() {
            let name = p.to_string_lossy().into_owned();
            let main_p = presets_path().join(&p).join("main.toml");
            p = if !main_p.exists() {
                presets_path().join(p.with_extension("toml"))
            } else {
                main_p
            };
            ensure_bundled_scripts(presets_path());
            if !p.exists() {
                embedded = embedded_preset(&name);
                if embedded.is_none() {
                    anyhow::bail!(missing_preset_message(&name, presets_path()));
                }
                log::debug!("Using built-in preset '{name}'");
            }
        }
        // no recursion because tail bad
        let o: PartialConfig = match embedded {
            Some(src) => parse_warn(src, &p)
                .map_err(|e| anyhow::anyhow!("Failed to parse built-in preset: {e}"))?,
            None => load_type(&p, |s| parse_warn(s, &p))?,
        };

        if let Some(q) = &o.source {
            let source = p.parent().as_ref().unwrap().join(q);
            let o: PartialConfig = load_type(&source, |s| parse_warn(s, &source))?;
            if o.source.is_some() {
                _wbog!("Ignoring 'source' field in nested override.");
            }
            config.apply(o);
        }

        config.apply(o);
        config.envs.insert(
            "WM_OVERRIDE".to_string(),
            EnvValue::new(p.to_string_lossy().to_string()),
        );
        config.envs.insert(
            "MM_OVERRIDE".to_string(),
            EnvValue::new(p.to_string_lossy().to_string()),
        );
    }

    warn_config_pitfalls(&config);

    if config.start.command.default.base_command.is_none() {
        config.start.command.default.base_command =
            Some(config.start.command.default.command.clone());
    }

    apply_path_rules(&mut config);

    if crate::paths::dev_mode() {
        config.tui.clear_on_exit = false;
    }
    apply_border_gradient_env(&mut config.render.ui.border);
    config.apply(partial); // resolve config.exit first
    config.resolve();

    if !cli.args.is_empty() {
        if !std::io::stdin().is_terminal() && !cli.no_read {
            eprintln!(
                "warning: trailing arguments provided but input is piped. ignoring trailing arguments."
            );
        }
        *COMMAND_ARGS.lock().unwrap() = cli.args;
    }

    // dispatch subcommands
    if cli.last_key {
        let path = config
            .exit
            .last_key_path
            .as_deref()
            .unwrap_or(last_key_path());

        let content = std::fs::read_to_string(path)._elog();
        if let Some(s) = content
            && let s = s.trim()
            && !s.is_empty()
        {
            prints!(s);
            exit(0);
        } else {
            exit(1)
        }
    }

    if cli.fullscreen {
        config.tui.layout = None;
    }

    if cli.sort {
        config.start.sort = true;
    }

    if cli.frecency {
        config.matcher.worker.frecency.active = true;
    }

    if cli.icons {
        config.render.results.icons = true;
    }

    if cli.symlink_target {
        config.render.results.symlink.active = true;
    }

    if let Some(pos) = cli.pos {
        config.render.results.pos = Some(pos);
    }

    // Synchronize [preview] media_size with [previewer] media_size if configured in TOML
    if let Some(size) = config.render.preview.media.size {
        config.previewer.media_size = size;
    }

    if let Some(props) = &cli.media {
        apply_media_props(props, &mut config);
    }

    if let Some(size_str) = &cli.media_size {
        apply_media_size_str(size_str, &mut config);
    }

    config.previewer.media = config.render.preview.media.active;
    config.previewer.markdown_diagrams = config.render.preview.diagrams.active;
    config.previewer.inline_diagrams = config.render.preview.diagrams.inline;
    config.previewer.inline_images = config.render.preview.inline_images;
    config.previewer.diagram_theme = config.render.preview.diagrams.theme;
    config.previewer.diagram_background = config.render.preview.diagrams.background;

    for spec in &cli.color {
        apply_color_spec(&mut config, spec);
    }

    if let Some(nav) = &cli.nav {
        apply_nav_props(nav, &mut config);
    }

    if cli.nav_hints {
        config.render.ui.nav.hints = true;
    }

    if cli.parent_peek {
        config.render.ui.parent_peek.enabled = true;
    }

    if cli.status_inline {
        config.render.query.status_inline = true;
    }

    for nb in &cli.nav_bind {
        if let Some(colon) = nb.find(':') {
            let key = nb[..colon].to_string();
            let action_str = &nb[colon + 1..];
            let parts = split_nav_bind_actions(action_str);

            let mut actions = waymaker::action::Actions::default();
            let mut parse_ok = true;
            for part in &parts {
                match part.parse::<waymaker::action::Action<waymaker::action::NullActionExt>>() {
                    Ok(action) => actions.push(action),
                    Err(e) => {
                        eprintln!("warning: invalid --nav-bind action '{}': {}", part, e);
                        parse_ok = false;
                        break;
                    }
                }
            }

            if parse_ok && !actions.is_empty() {
                config.render.ui.nav.binds.insert(key, actions);
            }
        } else {
            eprintln!(
                "warning: --nav-bind '{}' missing ':' separator (expected char:Action)",
                nb
            );
        }
    }

    if config.render.ui.nav.active {
        let profile = config.render.ui.nav.effective_profile();
        let defaults = waymaker::config::NavConfig::default_binds_for_profile(profile);
        for (k, v) in defaults {
            config.render.ui.nav.binds.entry(k).or_insert(v);
        }

        // In basic mode, ensure directory navigation binds h and l are silenced
        if profile == waymaker::config::NavProfile::Basic {
            let empty = waymaker::action::Actions::default();
            for key in &["h", "l"] {
                config
                    .render
                    .ui
                    .nav
                    .binds
                    .entry(key.to_string())
                    .or_insert_with(|| empty.clone());
            }
        }
    }

    if config.render.ui.nav.active
        && config.render.ui.nav.effective_profile() == waymaker::config::NavProfile::Fm
    {
        use waymaker::action::Actions;
        let mut nb = |k: &str, actions: Actions<waymaker::action::NullActionExt>| {
            config
                .render
                .ui
                .nav
                .binds
                .entry(k.to_string())
                .or_insert(actions);
        };
        nb("esc", waymaker::acs![Action::Quit(130)]);
        nb("q", waymaker::acs![Action::Quit(130)]);
        nb("d", waymaker::acs![Action::Semantic("fm_delete".into())]);
        nb("a", waymaker::acs![Action::Semantic("fm_create".into())]);
        nb("r", waymaker::acs![Action::Semantic("fm_rename".into())]);
        nb("z", waymaker::acs![Action::Semantic("fm_zip".into())]);
        nb("Z", waymaker::acs![Action::Semantic("fm_unzip".into())]);
        nb(" ", waymaker::acs![Action::Toggle]);
        nb("y", waymaker::acs![Action::Semantic("fm_yank".into())]);
        nb("Y", waymaker::acs![Action::Semantic("fm_unyank".into())]);
        nb("x", waymaker::acs![Action::Semantic("fm_cut".into())]);
        nb("X", waymaker::acs![Action::Semantic("fm_uncut".into())]);
        nb("p", waymaker::acs![Action::Semantic("fm_paste".into())]);
        nb(
            "P",
            waymaker::acs![Action::Semantic("fm_paste_into".into())],
        );
        nb("u", waymaker::acs![Action::Semantic("fm_undo".into())]);
        nb("ctrl-r", waymaker::acs![Action::Semantic("fm_redo".into())]);
        nb("D", waymaker::acs![Action::Semantic("fm_dragdrop".into())]);
        nb(",", waymaker::acs![Action::SortMenu]);
        nb("/", waymaker::acs![Action::FocusFilter]);
        nb("\\", waymaker::acs![Action::ToggleParentPeek]);
        nb("|", waymaker::acs![Action::ToggleParentPeek]);
        nb("f", waymaker::acs![Action::Semantic("reloadnext".into())]);
        nb("b", waymaker::acs![Action::Semantic("pins".into())]);
        nb("*", waymaker::acs![Action::Semantic("pin".into())]);
    }

    if cli.dump_config {
        let contents = toml::to_string_pretty(&config).expect("failed to serialize to TOML");

        // if piped: dump the current cfg
        std::io::Write::write_all(&mut std::io::stdout(), contents.as_bytes())?;

        exit(0);
    }

    config = finalize_binds(config)?;

    debug!("Config computed: {config:?}");

    Ok(config)
}

/// The start directory (`[start.directory]`): a path, or a script that prints one (`exec`).
/// With `force`, failing to switch is fatal.
fn enter_start_directory(directory: EnvValue, envs: &HashMap<String, String>) {
    if !directory.value.is_empty() {
        let EnvValue { value, force, exec } = directory;

        let mut failed = false;
        if exec {
            if let Some(new_d) = Command::from_script(&value)
                .envs(envs)
                .read_to_string()
                ._elog()
            {
                let new_d = Path::new(new_d.trim()).to_path_buf();
                if new_d.exists() {
                    failed = set_current_dir(&new_d)
                        .prefix(format!("Failed to switch to {new_d:?}"))
                        ._wbog()
                        .is_some();
                } else {
                    ebog!("Directory does not exist: {}", new_d.display());
                    failed = true;
                }
            } else {
                ebog!("Failed to execute script for directory: {}", value);
                failed = true;
            }
        } else {
            let path = expand_tilde(value.into());
            set_current_dir(&path)
                .prefix(format!("Failed to switch to {path:?}"))
                ._wbog();
        }

        if failed && force {
            std::process::exit(1);
        }
    }
}

/// All of the input at once, for `--sort`: stdin, the built-in walker, or the start command.
fn read_all_input(
    no_read: bool,
    command: &str,
    walker_config: &waymaker::config::WalkerConfig,
    envs: HashMap<String, String>,
) -> Vec<u8> {
    if !std::io::stdin().is_terminal() && !no_read {
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf).ok();
        buf
    } else if is_default_file_walker_command(command) {
        let walker = waymaker::walker::AsyncWalker::new(walker_config.to_options("."));
        let items = walker.collect_sync();
        items.join("\n").into_bytes()
    } else if !command.is_empty() {
        Command::from_script(command)
            .envs(envs)
            .args(&*COMMAND_ARGS.lock().unwrap())
            .output()
            .map(|o| o.stdout)
            .unwrap_or_default()
    } else {
        eprintln!("error: no input detected.");
        std::process::exit(99)
    }
}

/// Warns about config that silently does not do what it looks like: a column named like a
/// built-in template value, and a plain character bound under [binds].
fn warn_config_pitfalls(config: &Config) {
    // `{key}`, `{q}`, ... resolve to built-ins before columns: say so where the user sees it.
    for col in &config.columns.names {
        let name: &str = &col.name;
        if waymaker::config::RESERVED_TEMPLATE_NAMES.contains(&name) {
            wbog!(
                "Column '{name}' shares its name with a built-in template value: `{{{name}}}` will not expand to this column. Rename it."
            );
        }
    }

    // A bare printable key under [binds] fires while the query is typed: the character never
    // reaches the query (and `* = @bookmark` bookmarked whatever line was highlighted).
    // `"," = []` unbinds the key, which is fine.
    for (trigger, actions) in &config.binds {
        if !actions.0.is_empty()
            && let Some(c) = trigger.bare_printable_char()
        {
            wbog!(
                "[binds] '{trigger}' is a plain character: it runs its action instead of typing '{c}' into the query. Bind a chord (ctrl-…, F1) instead, or move it to [ui.nav.binds]."
            );
        }
    }
}

/// Applies the `[[rule]]`s whose path matches the working directory (their preset, then their
/// overrides), and the folder rules and default sort.
fn apply_path_rules(config: &mut Config) {
    // Apply matching directory / path rules (from config.toml and loaded presets)
    let current_dir = std::env::current_dir().unwrap_or_default();
    let rules = config.rule.clone();
    for rule in &rules {
        if rule.path.matches(&current_dir) {
            if let Some(ref p) = rule.preset {
                let mut preset_path = p.clone();
                if preset_path.is_relative() && preset_path.extension().is_none() {
                    let main_p = presets_path().join(&preset_path).join("main.toml");
                    preset_path = if !main_p.exists() {
                        presets_path().join(preset_path.with_extension("toml"))
                    } else {
                        main_p
                    };
                }
                if let Ok(preset_config) = load_type(&preset_path, |s| parse_warn(s, &preset_path))
                {
                    config.apply(preset_config);
                }
            }
            config.apply(rule.override_config.clone());
        }
    }

    if !config.folder_rules.is_empty() {
        config
            .render
            .ui
            .folder_rules
            .extend(config.folder_rules.clone());
    }
    if config.default_sort.is_some() {
        config.render.ui.default_sort = config.default_sort;
    }
}

/// Completes the binds: the defaults under the config's, `/` and shift-enter in navigation mode,
/// the built-in semantic aliases, then the cycle check and validation of CLI actions.
fn finalize_binds(mut config: Config) -> anyhow::Result<Config> {
    // check binds
    let slash_trigger = "/".parse().expect("slash trigger should parse");
    let user_has_slash = config.binds.contains_key(&slash_trigger);
    let user_has_focus_action = config.binds.values().any(|actions| {
        actions.iter().any(|a| {
            matches!(
                a,
                waymaker::Action::ToggleFocus
                    | waymaker::Action::FocusNav
                    | waymaker::Action::FocusFilter
            )
        })
    });

    config.binds = BindMap::default_binds().modify(|x| x.extend(config.binds));
    if config.render.ui.nav.active {
        if !user_has_slash && !user_has_focus_action {
            config
                .binds
                .insert(slash_trigger, waymaker::acs![waymaker::Action::FocusFilter]);
        }
        config.binds.insert(
            "shift-enter".parse().expect("shift-enter should parse"),
            waymaker::acs![
                waymaker::Action::Print("{=}".to_string()),
                waymaker::Action::Quit(2)
            ],
        );
    }

    let mut def_sem = |trigger_str: &str, action: Action<MMAction>| {
        if let Ok(t) = trigger_str.parse() {
            config.binds.entry(t).or_insert(waymaker::acs![action]);
        }
    };
    def_sem("@dirs", Action::Custom(MMAction::ReloadNext(Some(1))));
    def_sem("@frecency", Action::Custom(MMAction::ReloadNext(Some(1))));
    def_sem("@bookmarks", Action::Custom(MMAction::ReloadNext(Some(2))));
    def_sem("@bookmark", Action::Custom(MMAction::FmTogglePin));
    def_sem("@pin", Action::Custom(MMAction::FmTogglePin));
    def_sem("@pins", Action::Custom(MMAction::ReloadNext(Some(2))));
    def_sem("@local", Action::Custom(MMAction::ReloadNext(Some(0))));
    def_sem("@reloadnext", Action::Custom(MMAction::ReloadNext(None)));
    def_sem("@reloadprev", Action::Custom(MMAction::ReloadPrev));
    def_sem("@cycle", Action::Custom(MMAction::ReloadNext(None)));
    def_sem("@paste", Action::Custom(MMAction::FmPaste));
    def_sem("@paste_into", Action::Custom(MMAction::FmPasteInto));
    def_sem("@paste_target", Action::Custom(MMAction::FmPasteInto));
    define_used_builtin_aliases(&mut config.binds);

    config.binds.check_cycles().map_err(anyhow::Error::msg)?;
    config.binds.retain(|_, actions| !actions.is_empty());

    for actions in config.binds.values() {
        for a in actions {
            if let Action::Custom(mm) = &a {
                mm.validate()?;
            }
        }
    }
    Ok(config)
}

/// Defines each built-in alias (`@create`, `@rename`, `@delete`, ...) that a bind uses and the
/// config does not define itself. The event loop resolves `@alias` actions against the bind map
/// when it starts and drops every bind it cannot resolve, so without this a `[binds]` key bound
/// to a built-in file-manager alias would do nothing.
fn define_used_builtin_aliases(binds: &mut BindMap<MMAction>) {
    use waymaker::binds::{Trigger, TriggerKind};
    let used: Vec<String> = binds
        .values()
        .flat_map(|actions| actions.iter())
        .filter_map(|a| match a {
            Action::Semantic(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    for name in used {
        let defined = binds
            .keys()
            .any(|t| matches!(&t.kind, TriggerKind::Semantic(s) if *s == name));
        if !defined && let Some(actions) = handlers::builtin_alias(&name) {
            binds.insert(
                Trigger {
                    kind: TriggerKind::Semantic(name),
                    mode: String::new(),
                },
                actions,
            );
        }
    }
}

pub static COMMAND_ARGS: Mutex<Vec<std::ffi::OsString>> = Mutex::new(Vec::new());
/// Where the cursor goes after a directory change or reload. One per picker, shared by the
/// `ChDir`, `Reload` and `Synced` handlers and the fm actions.
#[derive(Debug, Default)]
pub struct NavMemory {
    /// Item to place the cursor on after the next sync. Seeded from an inherited
    /// `WM_TARGET_ITEM` (or legacy `MM_TARGET_ITEM`); after that it lives only here, since
    /// mutating the process environment races with other threads.
    pub target_item: Option<String>,
    /// The item, index and directory under the cursor when the last reload started.
    pub prev_reload_item: Option<(Option<String>, u32, std::path::PathBuf)>,
    /// Set by `ChDir`: the next reload starts at the top instead of restoring
    /// `prev_reload_item`.
    pub chdir_new_dir: bool,
}

pub type SharedNav = Arc<Mutex<NavMemory>>;

impl NavMemory {
    pub fn shared_from_env() -> SharedNav {
        Arc::new(Mutex::new(Self {
            target_item: std::env::var("WM_TARGET_ITEM")
                .or_else(|_| std::env::var("MM_TARGET_ITEM"))
                .ok(),
            ..Self::default()
        }))
    }
}

pub fn process_envs(mut envs: HashMap<String, EnvValue>) -> HashMap<String, String> {
    let mut processed_envs = HashMap::new();

    // todo: lowpri: should we provision this what is the cost of setting more env vars
    if !envs.contains_key("CLIPcmd") {
        if let Some(v) = std::env::var("CLIPcmd").ok()
            && !v.is_empty()
        {
            envs.insert("CLIPcmd".to_string(), EnvValue::new(v));
        } else {
            if let Some((clip, paste)) = guess_clip_cmd() {
                envs.insert("CLIPcmd".to_string(), EnvValue::new(clip));

                if !envs.contains_key("PASTEcmd")
                    && std::env::var("PASTEcmd").ok().is_none_or(|x| x.is_empty())
                {
                    envs.insert("PASTEcmd".to_string(), EnvValue::new(paste));
                }
            }
        }
    }

    if !envs.contains_key("PAGER") && std::env::var("PAGER").ok().is_none_or(|x| x.is_empty()) {
        let ev = EnvValue::new(guess_pager_cmd());
        envs.insert("PAGER".to_string(), ev);
    }

    if !envs.contains_key("EDITOR") && std::env::var("EDITOR").ok().is_none_or(|x| x.is_empty()) {
        let ev = EnvValue::new(guess_editor_cmd());
        envs.insert("EDITOR".to_string(), ev);
    }

    // First pass: static envs
    for (k, v) in &envs {
        if !v.value.is_empty() && !v.exec && (v.force || std::env::var_os(k).is_none()) {
            processed_envs.insert(k.clone(), v.value.to_string());
        }
    }

    // Second pass: dynamic envs
    for (k, v) in &envs {
        if !v.value.is_empty() && v.exec && (v.force || std::env::var_os(k).is_none()) {
            if let Some(output) = Command::from_script(&v.value)
                .envs(&processed_envs)
                .read_to_string()
                ._elog()
            {
                processed_envs.insert(k.clone(), output.trim().to_string());
            } else {
                _wbog!("Failed to execute env command for {}: {}", k, v.value);
            }
        }
    }

    processed_envs
}

pub async fn start(
    config: Config,
    no_read: bool,
    group_prefix: Option<String>,
) -> Result<(), MatchError> {
    let nav_mode = config.render.ui.nav.active;
    let nav_notify = config.render.ui.nav.notify;

    let Config {
        render,
        tui,
        previewer,
        matcher: MatcherConfig {
            matcher,
            mut worker,
        },
        columns,
        binds,
        start:
            StartConfig {
                input_separator,
                command:
                    StartCommandConfig {
                        default:
                            CommandSetting {
                                separator,
                                command,
                                base_command,
                            },
                        additional: mut additional_commands,
                    },
                directory,
                sync,
                output_separator,
                output_template,
                ansi,
                trim,
                mode,
                sort,
                reload_interval,
                shell,
                ..
            },
        walker: walker_config,
        mut exit,
        mut envs,
        source: _,
        rule: all_rules,
        ..
    } = config;

    let default_base_cmd = base_command
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| command.clone());

    if sort && !worker.sort.threshold.is_smart() {
        // Force nucleo to preserve insertion order (stable sort) so the alphabetically
        // sorted input is displayed in the same order when no query is typed.
        worker.sort.threshold = waymaker::config::SortThreshold::NEVER;
    }

    // -------- determine command ------------
    if let Some(first) = additional_commands.first_mut()
        && first.is_empty()
    {
        *first = command.clone();
    }
    let additional_commands = additional_commands;

    let mut initial_index = 0;
    if additional_commands.len() > 1
        && let Ok(index_str) = std::env::var("WM_INDEX").or_else(|_| std::env::var("MM_INDEX"))
        && let Ok(index) = index_str.parse::<usize>()
        && index < additional_commands.len()
    {
        initial_index = index;
    }

    let command = if initial_index > 0 {
        additional_commands[initial_index].clone()
    } else {
        command
    };

    let initial_cmd = if !command.is_empty() && std::io::stdin().is_terminal() || no_read {
        command.clone()
    } else {
        String::new()
    };

    // -------- set envs/directory -----------
    if !additional_commands.is_empty() {
        envs.insert(
            "WM_INDEX".to_string(),
            EnvValue::new(initial_index.to_string()),
        );
        envs.insert(
            "MM_INDEX".to_string(),
            EnvValue::new(initial_index.to_string()),
        );
    }
    let envs = process_envs(envs);

    if let Ok(cwd) = std::env::current_dir() {
        let store = waymaker::frecency::FrecencyStore::open();
        let _ = store.add(&cwd.to_string_lossy());
    }

    enter_start_directory(directory, &envs);

    // ---------------------------------

    let abort_empty = exit.abort_empty;
    let header_lines = render.header.header_lines;
    let print_handle = AppendOnly::new();
    let output_separator = output_separator.clone().unwrap_or("\n".into());
    let preprocess = (ansi, trim);

    if exit.last_key_path.is_none() {
        exit.last_key_path = Some(last_key_path().into())
    }

    let event_loop = EventLoop::with_binds(binds).with_tick_rate(render.tick_rate());

    // set event loop mode
    let mode = if let Some(m) = mode {
        m
    } else {
        match (
            !initial_cmd.is_empty(), // has command => t0
            std::io::stdout().is_terminal(),
        ) {
            (true, true) => "tty",
            (true, false) => "t0",
            (false, true) => "piped",
            (false, false) => "t1",
        }
        .to_string()
    };
    log::trace!("mode: {}", mode);
    event_loop.input_state().set_mode(mode);

    // make matcher and matchmaker with matchmaker-and-matcher-maker
    let copy_trailing_newline = tui.copy_trailing_newline;
    let (
        mut mm,
        injector,
        OddEnds {
            splitter,
            hidden_columns,
            has_error,
        },
    ) = Matchmaker::new_from_config(render, tui, worker, columns, exit, preprocess);

    if has_error {
        return Err(MatchError::Abort(1));
    }
    // make previewer

    if !event_loop.binds.check_traces() {
        // maybe abort with error
    }
    let cli_formatter = Either::Right(
        crate::formatter::format_cli
            as for<'a, 'b, 'c> fn(
                &'a MMState<'b, 'c, waymaker::ConfigMMItem, waymaker::ConfigMMInnerItem>,
                &'a str,
                Option<&dyn Fn(String)>,
            ) -> String,
    );
    let binds = event_loop.binds.clone();
    let semantic_binds = event_loop.semantic_binds.clone();
    let previewer = make_previewer(
        &mut mm,
        previewer,
        cli_formatter.clone(),
        Box::new(move |config, mode| waymaker::binds::display_help(&binds, config, Some(mode))),
    );

    // ---------------------- build options ---------------------------

    let bind_tx = event_loop.bind_controller();

    let envs_ = envs.clone();
    let mut options = PickOptions::new()
        .event_loop(event_loop)
        .matcher(matcher.0)
        .previewer(previewer)
        .hidden_columns(hidden_columns)
        .initializer(move |s| {
            s.envs.extend(envs_);
            s.picker_ui.query.set_mode_index(initial_index);
            s.picker_ui.results.set_mode_index(initial_index);
            s.picker_ui.worker.set_mode_index(initial_index);
        });

    let render_tx = options.render_tx();
    if initial_index > 0 {
        let _ = render_tx.send(waymaker::message::RenderCommand::Action(
            waymaker::action::Action::Custom(crate::action::MMAction::SetModeIndex(initial_index)),
        ));
    }
    let push_fn = inject_line(
        header_lines,
        render_tx.clone(),
        injector,
        group_prefix.clone(),
    );

    if let Some(interval) = reload_interval {
        let render_tx = render_tx.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(interval));
            ticker.tick().await; // skip first immediate tick
            loop {
                ticker.tick().await;
                if render_tx
                    .send(waymaker::message::RenderCommand::Action(
                        waymaker::action::Action::Reload("".to_string()),
                    ))
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    // ---------------------- register handlers ---------------------------
    // print handler (no quoting)
    mm._register_print_handler(
        print_handle.clone(),
        output_separator.clone(),
        cli_formatter.clone(),
    );

    // execute handlers
    mm.register_execute_handler(cli_formatter.clone());
    mm._register_execute_async_handler(cli_formatter.clone());
    mm._register_live_input_handler(cli_formatter.clone());
    mm.register_copy(
        cli_formatter.clone(),
        copy_trailing_newline,
        Some(render_tx.clone()),
    );
    mm._register_become_handler(cli_formatter.clone());
    // Speculative directory cache & background scanning
    let speculative_cache: Arc<Mutex<HashMap<std::path::PathBuf, Vec<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let nav = NavMemory::shared_from_env();

    mm.register_interrupt_handler(
        Interrupt::ChDir,
        chdir_handler(
            cli_formatter.clone(),
            speculative_cache.clone(),
            render_tx.clone(),
            nav.clone(),
        ),
    );

    mm.register_event_handler(
        Event::Synced | Event::Resynced,
        synced_handler(render_tx.clone(), nav.clone()),
    );

    let reload_rules_spec = all_rules.clone();
    let default_base_cmd_spec = default_base_cmd;
    let get_active_cmd = move |dir: &std::path::Path| -> String {
        active_command(&reload_rules_spec, &default_base_cmd_spec, dir)
    };

    mm.register_event_handler(
        Event::CursorChange,
        cursor_prefetch_handler(
            cli_formatter.clone(),
            speculative_cache.clone(),
            get_active_cmd.clone(),
            walker_config.clone(),
            separator.or(input_separator).unwrap_or('\n'),
        ),
    );

    mm.register_interrupt_handler(
        Interrupt::Reload,
        reload_handler(ReloadDeps {
            reload_formatter: cli_formatter.clone(),
            reload_render_tx: render_tx.clone(),
            get_active_cmd,
            spec_cache_reload: speculative_cache.clone(),
            walker_config_reload: walker_config.clone(),
            splitter,
            preprocess,
            group_prefix,
            separator,
            input_separator,
            nav: nav.clone(),
        }),
    );

    debug!("{mm:?}");

    let mut action_context = ActionContext {
        bind_tx,
        render_tx: render_tx.clone(),
        additional_commands: (additional_commands, initial_index),
        output_template,
        print_handle: print_handle.clone(),
        output_separator: output_separator.clone(),
        clipboard: Arc::new(Mutex::new(None)),
        fm_notify: nav_notify,
        undo_stack: Arc::new(Mutex::new(Vec::new())),
        redo_stack: Arc::new(Mutex::new(Vec::new())),
        fm_action: None,
        mode_history: std::collections::HashMap::new(),
        last_cwd: std::env::current_dir().ok(),
        nav,
    };

    options = options
        .ext_handler(move |x, y| action_handler(x, y, &mut action_context))
        .ext_aliaser(move |a, state| alias_action(a, &semantic_binds, &state.input_routing.mode()));

    if nav_mode {
        log::debug!("Navigation mode enabled");
    }

    // ----------- read -----------------------
    let handle = if sort {
        // Collect all input, sort alphabetically, then inject in sorted order.
        let sep = separator.or(input_separator).unwrap_or('\n');
        let raw: Vec<u8> = read_all_input(no_read, &command, &walker_config, envs);

        let text = String::from_utf8_lossy(&raw);
        let mut lines: Vec<&str> = text.split(sep).collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        lines.sort_unstable();
        let sorted = lines.join("\n");
        drop(lines);

        map_reader(
            std::io::Cursor::new(sorted),
            push_fn,
            None, // already newline-separated after join
            abort_empty.then_some(render_tx),
        )
    } else if !std::io::stdin().is_terminal() && !no_read {
        let stdin = std::io::stdin();
        map_reader(
            stdin,
            push_fn,
            input_separator,
            abort_empty.then_some(render_tx),
        )
    } else if is_default_file_walker_command(&command) {
        ingest_walk(push_fn, &walker_config)
    } else if !command.is_empty() {
        let mut cmd_builder = if let Some(sh) = shell.as_ref()
            && !sh.is_empty()
        {
            let mut iter = sh.iter();
            let mut program = Command::new(iter.next().unwrap());
            program.args(iter).arg(&command);
            program
        } else {
            Command::from_script(&command)
        };
        if let Some((mut _child, stdout)) = cmd_builder
            .envs(envs)
            .args(&*COMMAND_ARGS.lock().unwrap())
            .spawn_piped()
            ._ebog()
        {
            map_reader(
                stdout,
                push_fn,
                separator.or(input_separator),
                abort_empty.then_some(render_tx),
            )
        } else {
            eprintln!("error: no input detected.");
            std::process::exit(99)
        }
    } else {
        eprintln!("error: no input detected.");
        std::process::exit(99)
    };

    if sync {
        handle.await._wbog(); // warn the mapreader error (?)
    }

    let ret = mm.pick(options).await;

    print_handle.map_to_vec(|s| {
        log::trace!("{s}"); // this apparently helps with a race condition that erases output?
        print!("{}{}", s, output_separator);
    });

    log::trace!("Print complete");

    ret.map(|_| {})
}

pub fn is_target_item_match(val: &str, target: &str, is_local_mode: bool) -> bool {
    let target_trimmed = target.trim_end_matches('/');
    let val_trimmed = val.trim_end_matches('/');
    let val_is_abs = val_trimmed.starts_with('/') || val_trimmed.starts_with('\\');
    let target_is_abs = target_trimmed.starts_with('/') || target_trimmed.starts_with('\\');
    if val_trimmed == target_trimmed
        || val_trimmed.trim_start_matches("./") == target_trimmed.trim_start_matches("./")
    {
        true
    } else if val_is_abs && target_is_abs {
        false
    } else if !val_is_abs && target_is_abs {
        target_trimmed.ends_with(&format!("/{}", val_trimmed))
    } else if val_is_abs && !target_is_abs {
        is_local_mode && val_trimmed.ends_with(&format!("/{}", target_trimmed))
    } else {
        val_trimmed.ends_with(&format!("/{}", target_trimmed))
            || target_trimmed.ends_with(&format!("/{}", val_trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_media_size_str_and_sync() {
        let mut config = Config::default();
        apply_media_size_str("800", &mut config);
        assert_eq!(config.previewer.media_size, 800);
        assert_eq!(config.render.preview.media.size, Some(800));
        assert!(config.render.preview.media.active);

        apply_media_size_str("xl", &mut config);
        assert_eq!(config.previewer.media_size, 2048);
        assert_eq!(config.render.preview.media.size, Some(2048));

        apply_media_size_str("full", &mut config);
        assert_eq!(config.previewer.media_size, 0);
        assert_eq!(config.render.preview.media.size, Some(0));

        apply_media_props(&["size:1280".to_string()], &mut config);
        assert_eq!(config.previewer.media_size, 1280);
        assert_eq!(config.render.preview.media.size, Some(1280));
    }

    #[test]
    fn test_missing_preset_message_lists_available() {
        let dir = std::env::temp_dir().join(format!("wm_presets_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("git")).unwrap();
        std::fs::create_dir_all(dir.join("docker")).unwrap();
        std::fs::write(dir.join("rg.toml"), "").unwrap();
        std::fs::write(dir.join("jump.toml"), "").unwrap();
        std::fs::write(dir.join("notes.md"), "").unwrap();
        std::fs::write(dir.join("docker/main.toml"), "").unwrap();

        // `git` has no main.toml, so it is not a preset on its own.
        assert_eq!(available_presets(&dir), vec!["docker", "jump", "rg"]);
        let msg = missing_preset_message("jmp", &dir);
        assert!(msg.contains("Preset 'jmp' not found"));
        assert!(msg.contains(": docker, jump, rg\n"));
        assert!(msg.contains("Built in: cargo-rx, csv"));

        let empty = dir.join("git");
        assert!(missing_preset_message("jump", &empty).contains(": none\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_bundled_scripts_fill_gaps_and_keep_edits() {
        let dir = std::env::temp_dir().join(format!("wm-bundled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tmux")).unwrap();
        std::fs::write(dir.join("tmux/open.sh"), "edited").unwrap();
        ensure_bundled_scripts(&dir);
        assert_eq!(
            std::fs::read_to_string(dir.join("tmux/open.sh")).unwrap(),
            "edited"
        );
        for (rel, src) in BUNDLED_SCRIPTS.iter().filter(|(r, _)| *r != "tmux/open.sh") {
            assert_eq!(
                std::fs::read_to_string(dir.join(rel)).unwrap(),
                *src,
                "{rel}"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(dir.join(rel))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o111, 0o111, "{rel} not executable");
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_embedded_presets_parse() {
        for name in EMBEDDED_PRESETS {
            let src = embedded_preset(name).unwrap_or_else(|| panic!("{name} is not embedded"));
            let parsed: Result<PartialConfig, _> = toml::from_str(src);
            assert!(parsed.is_ok(), "built-in preset {name}: {:?}", parsed.err());
        }
        assert!(embedded_preset("nope").is_none());
    }

    #[tokio::test]
    async fn test_start_filter_headless() {
        let mut config = Config::default();
        config.start.command.default.command = "printf 'apple\\nbanana\\ncherry\\n'".to_string();
        let code = start_filter(config, "ban", true, None).await;
        assert_eq!(code, 0);

        let mut config_nomatch = Config::default();
        config_nomatch.start.command.default.command =
            "printf 'apple\\nbanana\\ncherry\\n'".to_string();
        let code_nomatch = start_filter(config_nomatch, "xyz", true, None).await;
        assert_eq!(code_nomatch, 1);
    }

    #[test]
    fn test_for_each_record_utf8_lossy_crlf_and_boundaries() {
        // Contains:
        // 1. Invalid UTF-8 byte (0xff) inside a record -> kept with U+FFFD
        // 2. CRLF line ending -> '\r' stripped when delim is '\n'
        // 3. Final record without trailing delimiter -> emitted
        let raw: &[u8] = b"alpha\nbad\xffbyte\nwindows\r\ntrailing";
        let mut out = Vec::new();
        let count = for_each_record::<()>(raw, b'\n', |rec| {
            out.push(rec.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 4);
        assert_eq!(out, vec!["alpha", "bad\u{FFFD}byte", "windows", "trailing"]);

        // NUL delimiter preserves '\r' and splits on 0x00.
        let nul_raw: &[u8] = b"one\r\0two\xff\0three\0";
        let mut nul_out = Vec::new();
        let nul_count = for_each_record::<()>(nul_raw, b'\0', |rec| {
            nul_out.push(rec.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(nul_count, 3);
        assert_eq!(nul_out, vec!["one\r", "two\u{FFFD}", "three"]);

        // Record spanning across the 256 KiB BufReader capacity.
        let long = "x".repeat(300 * 1024);
        let payload = format!("{long}\nafter\n");
        let mut span_out = Vec::new();
        let span_count = for_each_record::<()>(payload.as_bytes(), b'\n', |rec| {
            span_out.push(rec.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(span_count, 2);
        assert_eq!(span_out[0].len(), 300 * 1024);
        assert_eq!(span_out[1], "after");
    }

    #[tokio::test]
    async fn test_start_filter_keeps_records_after_invalid_utf8() {
        let mut config = Config::default();
        config.start.command.default.command =
            "printf 'first\\nbad\\377line\\ntarget_match\\n'".to_string();
        let code = start_filter(config, "target_match", true, None).await;
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn test_start_benchmark_input() {
        let mut config = Config::default();
        config.start.command.default.command = "printf 'alpha\\nbeta\\ngamma\\n'".to_string();
        // no_read = true: run the command above. With false, a non-tty stdin that never
        // closes (cargo test started from a pipe) is read instead, and the test hangs.
        let code = start_benchmark_input(config, "bet", true, None).await;
        assert_eq!(code, 0);
    }
}
