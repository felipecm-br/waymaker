use clap::CommandFactory;
use clap_complete::{Shell, generate_to};
use std::env;

// include!("build/completions_mock.rs");

// -----------------------------------------------------------------------------
// Include
// -----------------------------------------------------------------------------
include!("src/clap.rs");

fn generate_completions() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = manifest_dir.join("assets").join("completions");
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut cmd = Cli::command();
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell] {
        generate_to(shell, &mut cmd, BINARY_SHORT, &out_dir).unwrap();
    }
}

/// Writes `$OUT_DIR/win.config.toml`: `assets/config.toml` with the values in
/// `assets/win.overrides.toml` written over it. Comments and layout come from config.toml, so
/// Windows gets every change to the default config; an override for a key config.toml does not
/// have fails the build instead of being silently ignored.
fn generate_windows_config() {
    use toml_edit::{DocumentMut, Item, Table};

    fn overlay(dst: &mut Table, src: &Table, path: &str) {
        for (key, item) in src.iter() {
            let path = if path.is_empty() {
                key.to_string()
            } else {
                format!("{path}.{key}")
            };
            let Some(slot) = dst.get_mut(key) else {
                panic!(
                    "assets/win.overrides.toml sets `{path}`, which assets/config.toml does not have"
                );
            };
            match (slot, item) {
                (Item::Table(dst), Item::Table(src)) => overlay(dst, src, &path),
                (Item::Value(dst), Item::Value(src)) => {
                    let decor = dst.decor().clone();
                    *dst = src.clone();
                    *dst.decor_mut() = decor;
                }
                _ => panic!(
                    "assets/win.overrides.toml: `{path}` has a different type in assets/config.toml"
                ),
            }
        }
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets = manifest_dir.join("assets");
    let read = |name: &str| -> DocumentMut {
        println!("cargo:rerun-if-changed=assets/{name}");
        std::fs::read_to_string(assets.join(name))
            .unwrap()
            .parse()
            .unwrap_or_else(|e| panic!("assets/{name}: {e}"))
    };
    let mut config = read("config.toml");
    let overrides = read("win.overrides.toml");
    overlay(config.as_table_mut(), overrides.as_table(), "");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("win.config.toml");
    std::fs::write(out, config.to_string()).unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=src/clap.rs");
    println!("cargo:rerun-if-changed=build.rs");

    generate_windows_config();

    if env::var_os("SKIP_COMPLETIONS").is_none() {
        generate_completions();
    }
}
