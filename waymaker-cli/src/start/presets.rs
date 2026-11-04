use super::*;

/// Top-level presets in `dir`: `<name>.toml` files and `<name>/main.toml` folders.
pub(super) fn available_presets(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                path.join("main.toml")
                    .exists()
                    .then(|| entry.file_name().to_string_lossy().into_owned())
            } else if path.extension().is_some_and(|ext| ext == "toml") {
                path.file_stem().map(|s| s.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

/// Names of the presets compiled into the binary; see [`embedded_preset`].
pub(super) const EMBEDDED_PRESETS: &[&str] = &[
    "cargo-rx",
    "csv",
    "downloads",
    "ftb",
    "jump",
    "kill",
    "man",
    "memory",
    "nvim",
    "pr",
    "ps",
    "rg",
    "session-picker",
    "tmux/keybindings",
    "tmux/scrollback",
    "workspace",
];

/// Scripts the bundled presets call through `"$(dirname -- "$MM_OVERRIDE")"`, by path under
/// the presets directory; see [`ensure_bundled_scripts`].
pub(super) const BUNDLED_SCRIPTS: &[(&str, &str)] = &[
    (
        "rg/toggle-case.sh",
        include_str!("../../assets/presets/rg/toggle-case.sh"),
    ),
    (
        "rg/toggle-word.sh",
        include_str!("../../assets/presets/rg/toggle-word.sh"),
    ),
    (
        "tmux/keybindings.sh",
        include_str!("../../assets/presets/tmux/keybindings.sh"),
    ),
    (
        "tmux/open.sh",
        include_str!("../../assets/presets/tmux/open.sh"),
    ),
    (
        "tmux/scrollback.sh",
        include_str!("../../assets/presets/tmux/scrollback.sh"),
    ),
];

/// Writes the bundled scripts that are missing under `dir` (the presets directory), so the
/// built-in presets that call them work before `install.sh` or `wm --download` has run. A file
/// already there, edited or not, is kept.
pub(super) fn ensure_bundled_scripts(dir: &Path) {
    for (rel, src) in BUNDLED_SCRIPTS {
        let path = dir.join(rel);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            continue;
        }
        if std::fs::write(&path, src).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
            }
        }
    }
}

/// Core presets compiled into the binary. They are used when no preset of that
/// name is installed, so `wm -o jump` works before `install.sh` or
/// `wm --download` has run; an installed file always takes precedence.
pub(super) fn embedded_preset(name: &str) -> Option<&'static str> {
    Some(match name {
        "cargo-rx" => include_str!("../../assets/presets/cargo-rx.toml"),
        "csv" => include_str!("../../assets/presets/csv.toml"),
        "downloads" => include_str!("../../assets/presets/downloads.toml"),
        "ftb" => include_str!("../../assets/presets/ftb.toml"),
        "jump" => include_str!("../../assets/presets/jump.toml"),
        "kill" => include_str!("../../assets/presets/kill.toml"),
        "man" => include_str!("../../assets/presets/man.toml"),
        "memory" => include_str!("../../assets/presets/memory.toml"),
        "nvim" => include_str!("../../assets/presets/nvim.toml"),
        "pr" => include_str!("../../assets/presets/pr.toml"),
        "ps" => include_str!("../../assets/presets/ps.toml"),
        "rg" if cfg!(windows) => include_str!("../../assets/presets/win.rg.toml"),
        "rg" => include_str!("../../assets/presets/rg.toml"),
        "session-picker" => include_str!("../../assets/presets/session-picker.toml"),
        "tmux/keybindings" => include_str!("../../assets/presets/tmux/keybindings.toml"),
        "tmux/scrollback" => include_str!("../../assets/presets/tmux/scrollback.toml"),
        "workspace" => include_str!("../../assets/presets/workspace.toml"),
        _ => return None,
    })
}

pub(super) fn missing_preset_message(name: &str, dir: &Path) -> String {
    let installed = available_presets(dir);
    let installed = if installed.is_empty() {
        "none".to_string()
    } else {
        installed.join(", ")
    };
    format!(
        "Preset '{name}' not found.\nInstalled in {}: {installed}\nBuilt in: {}\nRun `wm --download` to install or update the bundled presets.",
        dir.display(),
        EMBEDDED_PRESETS.join(", ")
    )
}
