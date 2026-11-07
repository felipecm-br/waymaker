//! Property tests for the `-o path=value` override pipeline: get_pairs, the `|||` split,
//! try_split_kv, and the macro-generated `PartialConfig::set` and `Config::apply`.
//! Random input may be rejected with an error, but must never panic.

use crate::config::Config;
use proptest::prelude::*;
use waymaker_partial::Apply;

/// Every leaf path of the default config (`results.icons`, `preview.layout`, ...), from its TOML.
fn config_paths() -> Vec<String> {
    fn walk(prefix: &str, table: &toml::Table, out: &mut Vec<String>) {
        for (k, v) in table {
            let path = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            match v {
                toml::Value::Table(t) if !t.is_empty() => walk(&path, t, out),
                _ => out.push(path),
            }
        }
    }
    let text = toml::to_string_pretty(&Config::default()).unwrap();
    let table: toml::Table = text.parse().unwrap();
    let mut out = Vec::new();
    walk("", &table, &mut out);
    // Environment overrides are refused on purpose; they would only test that refusal.
    out.retain(|p| !p.starts_with("env"));
    out
}

/// Values shaped like what users type: bools, numbers, colours, words, `k=v` lists and `|||`.
fn value() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("true".into()),
        Just("false".into()),
        (-5i64..300).prop_map(|n| n.to_string()),
        Just("red".into()),
        Just("#1e1e2e".into()),
        Just("50%".into()),
        Just("Accept".into()),
        "[a-z]{1,6}=[a-z0-9]{0,6}",
        "[a-z]{1,6}=[a-z0-9]{0,4} [a-z]{1,6}=[a-z0-9]{0,4}",
        "[a-zA-Z0-9 ]{1,8}\\|\\|\\|[a-zA-Z0-9 ]{1,8}",
        "\\PC{0,16}",
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Arbitrary words after `-o`: an error is fine, a panic is not.
    #[test]
    fn overrides_never_panic(words in prop::collection::vec("\\PC{0,24}", 0..6)) {
        let _ = crate::get_partial(words);
    }

    /// Every real config path with a typed-looking value: parsing, the generated `set`, and
    /// merging the result over the defaults never panic.
    #[test]
    fn every_config_path_accepts_or_rejects_without_panicking(
        idx in any::<prop::sample::Index>(),
        value in value(),
        as_one_word in any::<bool>(),
    ) {
        let paths = config_paths();
        let path = &paths[idx.index(paths.len())];
        let words = if as_one_word {
            vec![format!("{path}={value}")]
        } else {
            vec![path.clone(), value]
        };
        if let Ok(partial) = crate::get_partial(words) {
            let mut config = Config::default();
            config.apply(partial);
        }
    }

    /// Bind overrides with any key, including ones containing '=' (`b.alt-==Accept`).
    #[test]
    fn bind_overrides_never_panic(key in "\\PC{0,8}", action in "\\PC{0,16}") {
        if let Ok(partial) = crate::get_partial(vec![format!("b.{key}={action}")]) {
            let mut config = Config::default();
            config.apply(partial);
        }
    }
}

#[test]
fn config_paths_cover_the_main_sections() {
    let paths = config_paths();
    for section in ["results.", "preview.", "query.", "matcher.", "start."] {
        assert!(
            paths.iter().any(|p| p.starts_with(section)),
            "no {section} paths in {} collected",
            paths.len()
        );
    }
}
