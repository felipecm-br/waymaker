use super::*;

pub(super) fn is_default_file_walker_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    trimmed.is_empty()
        || (trimmed.contains("fd") && trimmed.contains("find"))
        || trimmed == "fd --strip-cwd-prefix --print0"
        || trimmed == "find . -print0"
}

/// Headless filter execution: reads input lines, feeds them to the matcher,
/// runs the query, prints matched items in ranked order to stdout, and exits.
pub async fn start_filter(
    config: Config,
    query: &str,
    no_read: bool,
    group_prefix: Option<String>,
) -> i32 {
    use std::io::Write;

    let Config {
        render,
        tui,
        matcher: MatcherConfig { worker, .. },
        columns,
        start:
            StartConfig {
                input_separator,
                command:
                    StartCommandConfig {
                        default: CommandSetting { command, .. },
                        ..
                    },
                directory,
                ansi,
                trim,
                ..
            },
        walker: walker_config,
        exit,
        envs,
        ..
    } = config;

    let envs = process_envs(envs);

    if !directory.value.is_empty() {
        let path = expand_tilde(directory.value.into());
        let _ = set_current_dir(&path);
    }

    let preprocess = (ansi, trim);
    let (mut mm, injector, OddEnds { has_error, .. }) =
        Matchmaker::new_from_config(render, tui, worker, columns, exit, preprocess);

    if has_error {
        return 1;
    }

    let notify = std::sync::Arc::new(tokio::sync::Notify::new());
    let notify_cb = notify.clone();
    mm.worker.set_notify(move || {
        notify_cb.notify_one();
    });
    mm.worker.find(query);

    let handle = if !std::io::stdin().is_terminal() && !no_read {
        let stdin = std::io::stdin();
        map_reader_batched(stdin, injector.clone(), group_prefix, input_separator, None)
    } else if is_default_file_walker_command(&command) {
        let walker_opts = walker_config.to_options(".");
        let walker = waymaker::walker::AsyncWalker::new(walker_opts);
        let h = walker.spawn_walk_parallel(injector.clone());
        tokio::spawn(async move {
            let _ = h.await;
            Ok(0)
        })
    } else if !command.is_empty() {
        let mut cmd_builder = Command::from_script(&command);
        if let Some((_child, stdout)) = cmd_builder
            .envs(envs)
            .args(&*COMMAND_ARGS.lock().unwrap())
            .spawn_piped()
            ._elog()
        {
            map_reader_batched(
                stdout,
                injector.clone(),
                group_prefix,
                input_separator,
                None,
            )
        } else {
            tokio::spawn(async { Ok(0) })
        }
    } else {
        tokio::spawn(async { Ok(0) })
    };

    // Match while input is still streaming, waking on injector/matcher notify
    // instead of spinning in a tight loop when nucleo is idle between batches.
    let mut handle = std::pin::pin!(handle);
    loop {
        while mm.worker.nucleo.tick(10).running {}
        tokio::select! {
            biased;
            _ = &mut handle => break,
            _ = notify.notified() => {}
        }
    }

    // Drain any items injected by the final batch before `handle` finished.
    while mm.worker.nucleo.tick(10).running {}

    let code = {
        let items = mm.worker.get_all_sorted();

        if items.is_empty() {
            1
        } else {
            let stdout = std::io::stdout();
            let mut out = std::io::BufWriter::new(stdout.lock());
            for item in items {
                if mm.worker.columns.len() == 1 {
                    let _ = writeln!(out, "{}", mm.worker.columns[0].raw(item));
                } else {
                    let raw = mm
                        .worker
                        .columns
                        .iter()
                        .map(|col| col.raw(item))
                        .collect::<Vec<_>>()
                        .join("\t");
                    let _ = writeln!(out, "{}", raw);
                }
            }
            let _ = out.flush();
            0
        }
    };

    // The caller exits right after this. Freeing every injected item first
    // costs tens of milliseconds on large inputs, so leave that to the OS.
    std::mem::forget(mm);
    code
}
