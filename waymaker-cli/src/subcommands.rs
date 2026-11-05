//! Bodies of the `wm <subcommand>` arms of [`handle_frecency_cli`](super::handle_frecency_cli).
//! Each takes the subcommand's arguments (`args[0]` is the subcommand) and returns the exit
//! code, or `None` when `wm` should go on to start the picker.

use super::*;

/// `wm init <shell>`: prints the shell integration script.
pub(crate) fn cmd_init(args: &[String]) -> Option<i32> {
    let shell = args.get(1).map(|s| s.as_str()).unwrap_or("zsh");
    let mut cmd_name = "z";
    for (idx, arg) in args.iter().enumerate() {
        if arg == "--cmd" {
            if let Some(val) = args.get(idx + 1) {
                cmd_name = val.as_str();
            }
        } else if let Some(val) = arg.strip_prefix("--cmd=") {
            cmd_name = val;
        }
    }

    let raw_script = match shell {
        "zsh" => include_str!("shell/zsh.sh"),
        "bash" => include_str!("shell/bash.sh"),
        "fish" => include_str!("shell/fish.fish"),
        "nushell" | "nu" => include_str!("shell/nu.nu"),
        "powershell" | "pwsh" => include_str!("shell/pwsh.ps1"),
        _ => {
            eprintln!(
                "Unsupported shell '{shell}'. Supported shells: zsh, bash, fish, nushell, powershell"
            );
            return Some(1);
        }
    };

    let mut script = if cmd_name != "z" {
        raw_script
            .replace("z()", &format!("{cmd_name}()"))
            .replace("zi()", &format!("{cmd_name}i()"))
            .replace("function z\n", &format!("function {cmd_name}\n"))
            .replace("function zi\n", &format!("function {cmd_name}i\n"))
            .replace("def --env z ", &format!("def --env {cmd_name} "))
            .replace("function z ", &format!("function {cmd_name} "))
    } else {
        raw_script.to_string()
    };

    if cmd_name != "z" {
        script.push_str(&format!(
            "\nalias z={cmd_name} 2>/dev/null\nalias zi={cmd_name}i 2>/dev/null\n"
        ));
    }

    println!("{script}");
    Some(0)
}

/// `wm list` / `wm query`: prints frecency entries (directories, files, pins), ranked.
pub(crate) fn cmd_list(args: &[String]) -> Option<i32> {
    let dirs_only = args
        .iter()
        .any(|a| a == "-d" || a == "--dirs" || a == "--dirs-only");
    let pins_only = args.iter().any(|a| {
        a == "-p"
            || a == "--pins"
            || a == "--pins-only"
            || a == "--bookmarks"
            || a == "--bookmarks-only"
    });
    let keywords: Vec<String> = args
        .iter()
        .skip(1)
        .filter(|a| {
            *a != "-l"
                && *a != "--list"
                && *a != "-d"
                && *a != "--dirs"
                && *a != "--dirs-only"
                && *a != "-p"
                && *a != "--pins"
                && *a != "--pins-only"
                && *a != "--bookmarks"
                && *a != "--bookmarks-only"
                && !a.starts_with('-')
        })
        .map(|a| a.to_lowercase())
        .collect();

    let full_query = keywords.join(" ");
    let store = waymaker::frecency::FrecencyStore::open();
    if !pins_only
        && dirs_only
        && let Ok(cwd) = std::env::current_dir()
    {
        let _ = store.add(&cwd.to_string_lossy());
    }
    let pinned_paths = store.list_pins();
    let pins_set: std::collections::HashSet<String> = pinned_paths.iter().cloned().collect();

    if pins_only {
        for path in pinned_paths {
            let path_obj = std::path::Path::new(&path);
            if dirs_only && !path_obj.is_dir() {
                continue;
            }
            let lower_path = path.to_lowercase();
            let matches_all = keywords.iter().all(|kw| lower_path.contains(kw));
            if matches_all {
                println!("{path}");
            }
        }
        return Some(0);
    }

    let mut pinned_matches: Vec<(String, usize, bool)> = Vec::new();
    for path in &pinned_paths {
        let path_obj = std::path::Path::new(path);
        if dirs_only && !path_obj.is_dir() {
            continue;
        }
        let lower_path = path.to_lowercase();
        let matches_all = keywords.iter().all(|kw| lower_path.contains(kw));
        if matches_all {
            let ends_with_query = !full_query.is_empty()
                && (lower_path.ends_with(&full_query)
                    || keywords.last().is_some_and(|lk| lower_path.ends_with(lk)));
            let path_depth = path_obj.components().count();
            pinned_matches.push((path.clone(), path_depth, ends_with_query));
        }
    }

    pinned_matches.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));

    let snapshot = store.get_snapshot();
    let mut matches: Vec<(String, u32, usize, bool)> = Vec::new();

    for (path, score) in snapshot.scores {
        if pins_set.contains(&path) {
            continue;
        }
        let path_obj = std::path::Path::new(&path);
        if dirs_only && !path_obj.is_dir() {
            continue;
        }

        let lower_path = path.to_lowercase();
        let matches_all = keywords.iter().all(|kw| lower_path.contains(kw));
        if matches_all {
            let ends_with_query = !full_query.is_empty()
                && (lower_path.ends_with(&full_query)
                    || keywords.last().is_some_and(|lk| lower_path.ends_with(lk)));
            let path_depth = path_obj.components().count();
            matches.push((path, score, path_depth, ends_with_query));
        }
    }

    // Sort:
    // 1. Exact/tail match bonus (ends_with_query = true first)
    // 2. Frecency score descending
    // 3. Shorter path depth
    matches.sort_by(|a, b| {
        b.3.cmp(&a.3)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    // Output pins first (Tier 0), then frecency matches (Tier 1)
    for (path, _, _) in pinned_matches {
        println!("{path}");
    }
    for (path, _, _, _) in matches {
        println!("{path}");
    }
    Some(0)
}

/// `wm mermaid <file>`: renders a Mermaid diagram to the terminal.
pub(crate) fn cmd_mermaid(args: &[String]) -> Option<i32> {
    let text_only = args.iter().any(|a| a == "--text");
    let ascii = args.iter().any(|a| a == "--ascii");
    let width = parse_preview_width_from_args(args);
    let theme = parse_theme_from_args(args);
    let bg = parse_bg_from_args(args);
    let path_arg = parse_subcommand_path_arg(args);
    let content = if let Some(p) = path_arg {
        if p == "-" {
            let mut buf = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf);
            buf
        } else {
            std::fs::read_to_string(p).unwrap_or_else(|e| format!("Error reading {p}: {e}"))
        }
    } else {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            let mut buf = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf);
            buf
        } else {
            eprintln!(
                "Usage: wm mermaid <file.mmd> [--width <N>] [--text] [--ascii] [--theme <auto|dark|light>] [--bg <transparent|solid>]"
            );
            return Some(1);
        }
    };
    if !ascii
        && !text_only
        && waymaker::utils::mermaid::is_kitty_supported()
        && let Some(diag) =
            waymaker::utils::mermaid::render_mermaid_to_unicode_placeholders_with_options(
                &content, width, theme, bg,
            )
    {
        print!("{}", diag.transmission);
        let mut lines = Vec::with_capacity(diag.lines.len() + 2);
        lines.push(ratatui::text::Line::default());
        lines.extend(diag.lines);
        lines.push(ratatui::text::Line::default());
        let text = ratatui::text::Text::from(lines);
        let ansi_output = waymaker::utils::text::text_to_ansi(&text);
        if ansi_output.ends_with('\n') {
            print!("{ansi_output}");
        } else {
            println!("{ansi_output}");
        }
        return Some(0);
    }
    let opts = waymaker::utils::mermaid::MermaidOptions {
        max_width: width,
        ascii,
        show_box: true,
        title: Some("Mermaid Diagram".to_string()),
        inline_diagrams: false,
        theme,
        background: bg,
    };
    let ansi_output = waymaker::utils::mermaid::render_mermaid_ansi(&content, &opts);
    if ansi_output.ends_with('\n') {
        print!("{ansi_output}");
    } else {
        println!("{ansi_output}");
    }
    Some(0)
}

/// `wm md <file>`: renders Markdown (with Mermaid blocks) to the terminal.
pub(crate) async fn cmd_markdown(args: &[String]) -> Option<i32> {
    let watch = args.iter().any(|a| a == "-w" || a == "--watch");
    let text_only = args.iter().any(|a| a == "--text");
    let ascii = args.iter().any(|a| a == "--ascii");
    let no_mermaid = args
        .iter()
        .any(|a| a == "--no-mermaid" || a == "--no-diagrams");
    let no_images = args.iter().any(|a| a == "--no-images");
    let width = parse_preview_width_from_args(args);
    let theme = parse_theme_from_args(args);
    let bg = parse_bg_from_args(args);
    let path_arg = parse_subcommand_path_arg(args);

    if watch {
        if path_arg.is_none() || path_arg == Some("-") {
            eprintln!("Error: --watch requires a file path on disk, stdin cannot be watched");
            return Some(1);
        }
        let p = path_arg.unwrap();
        let target_path = std::path::Path::new(p);
        if !target_path.exists() {
            eprintln!("Error: file '{}' not found", p);
            return Some(1);
        }
        if target_path.is_dir() {
            eprintln!("Error: '{}' is a directory, not a markdown file", p);
            return Some(1);
        }
        let abs_target = if target_path.is_absolute() {
            target_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|c| c.join(target_path))
                .unwrap_or_else(|_| target_path.to_path_buf())
        };
        let dynamic_width = width.is_none();
        let opts = waymaker::utils::markdown::MarkdownOptions {
            max_width: width,
            base_path: Some(abs_target),
            render_mermaid: !no_mermaid,
            mermaid_ascii: ascii,
            show_line_numbers: false,
            mermaid_image: !ascii && !text_only && !no_mermaid,
            inline_diagrams: !ascii && !text_only && !no_mermaid,
            inline_images: !ascii && !text_only && !no_images,
            diagram_theme: theme,
            diagram_background: bg,
        };
        if let Err(e) = crate::watch::watch_markdown(target_path, opts, dynamic_width).await {
            eprintln!("Error during watch: {e}");
            return Some(1);
        }
        return Some(0);
    }

    let content = if let Some(p) = path_arg {
        if p == "-" {
            let mut buf = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf);
            buf
        } else {
            let target_path = std::path::Path::new(p);
            if !target_path.exists() {
                eprintln!("Error: file '{}' not found", p);
                return Some(1);
            }
            if target_path.is_dir() {
                eprintln!("Error: '{}' is a directory, not a markdown file", p);
                return Some(1);
            }
            std::fs::read_to_string(p).unwrap_or_else(|e| format!("Error reading {p}: {e}"))
        }
    } else {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            let mut buf = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf);
            buf
        } else {
            eprintln!(
                "Usage: wm md <file.md> [-w|--watch] [--width <N>] [--text] [--ascii] [--no-mermaid] [--no-images] [--theme <auto|dark|light>] [--bg <transparent|solid>]"
            );
            return Some(1);
        }
    };
    let opts = waymaker::utils::markdown::MarkdownOptions {
        max_width: width,
        base_path: path_arg.filter(|p| *p != "-").map(|p| {
            let path = std::path::Path::new(p);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map(|c| c.join(path))
                    .unwrap_or_else(|_| path.to_path_buf())
            }
        }),
        render_mermaid: !no_mermaid,
        mermaid_ascii: ascii,
        show_line_numbers: false,
        mermaid_image: !ascii && !text_only && !no_mermaid,
        inline_diagrams: !ascii && !text_only && !no_mermaid,
        inline_images: !ascii && !text_only && !no_images,
        diagram_theme: theme,
        diagram_background: bg,
    };
    let ansi_output = waymaker::utils::markdown::render_markdown_ansi(&content, &opts);
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() {
        let (sync_start, sync_end) = crate::watch::get_sync_update_delimiters();
        print!("{sync_start}");
        if ansi_output.ends_with('\n') {
            print!("{ansi_output}");
        } else {
            println!("{ansi_output}");
        }
        print!("{sync_end}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    } else {
        if ansi_output.ends_with('\n') {
            print!("{ansi_output}");
        } else {
            println!("{ansi_output}");
        }
    }
    Some(0)
}
