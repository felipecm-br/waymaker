use super::*;

/// Benchmark continuous streaming frametime on Ratatui test backend.
pub async fn start_benchmark_input(
    config: Config,
    query: &str,
    no_read: bool,
    group_prefix: Option<String>,
) -> i32 {
    let Config {
        render,
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
    let (mut mm, injector, OddEnds { has_error, .. }) = Matchmaker::new_from_config(
        render.clone(),
        waymaker::config::TerminalConfig::default(),
        worker,
        columns,
        exit,
        preprocess,
    );

    if has_error {
        return 1;
    }

    // Launch streaming ingestion
    let ingestion_handle = if !std::io::stdin().is_terminal() && !no_read {
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

    let width = 120u16;
    let height = 30u16;
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).expect("Failed to initialize TestBackend");

    mm.worker.find(query);

    let mut matcher =
        waymaker::nucleo::nucleo::Matcher::new(waymaker::nucleo::nucleo::Config::DEFAULT);
    let mut picker = waymaker::ui::PickerUI::new(
        render.results,
        render.status,
        render.query,
        render.header,
        render.action,
        render.breadcrumb,
        &mut matcher,
        mm.worker,
        mm.selector,
    );
    picker.query.push_str(query);

    let mut frametimes_ms: Vec<f64> = Vec::new();
    let mut click = waymaker::render::Click::None;
    let frame_area = ratatui::layout::Rect::new(0, 0, width, height);

    let target_frame_budget = std::time::Duration::from_micros(16_667); // 60 FPS
    let mut finished_ingestion = false;
    let mut consecutive_idle_ticks = 0;
    let bench_start = std::time::Instant::now();

    while consecutive_idle_ticks < 10 {
        let frame_start = std::time::Instant::now();

        // 1. Tick matching engine
        let status = picker.worker.nucleo.tick(10);
        picker.worker.find(query);
        let (_, snap_status) = waymaker::nucleo::Worker::new_snapshot(&mut picker.worker.nucleo);
        picker.results.status = snap_status;

        // 2. Render frame to TestBackend and measure frametime
        let render_start = std::time::Instant::now();
        terminal
            .draw(|frame| {
                let [_breadcrumb, _action, _input, _status, _header, results_area] =
                    picker.layout(frame_area, false);
                let (widget, _) = picker.make_table(&mut click, None, false);
                frame.render_widget(widget, results_area);
            })
            .expect("Terminal draw failed");
        let render_elapsed = render_start.elapsed();

        frametimes_ms.push(render_elapsed.as_secs_f64() * 1000.0);

        if !finished_ingestion && ingestion_handle.is_finished() {
            finished_ingestion = true;
        }

        if finished_ingestion && !status.running {
            consecutive_idle_ticks += 1;
        } else {
            consecutive_idle_ticks = 0;
        }

        let total_frame = frame_start.elapsed();
        if total_frame < target_frame_budget {
            tokio::time::sleep(target_frame_budget - total_frame).await;
        } else {
            tokio::task::yield_now().await;
        }
    }

    frametimes_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count = frametimes_ms.len();
    if count == 0 {
        eprintln!("Error: no frames rendered.");
        return 1;
    }

    let min_ms = frametimes_ms[0];
    let max_ms = frametimes_ms[count - 1];
    let p50_ms = frametimes_ms[(count as f64 * 0.50) as usize];
    let p90_ms = frametimes_ms[(count as f64 * 0.90).min((count - 1) as f64) as usize];
    let p95_ms = frametimes_ms[(count as f64 * 0.95).min((count - 1) as f64) as usize];
    let p99_ms = frametimes_ms[(count as f64 * 0.99).min((count - 1) as f64) as usize];

    let total_elapsed = bench_start.elapsed();
    let matches = picker.results.status.matched_count;
    let total_items = picker.results.status.item_count;

    println!("=== Waymaker Continuous Streaming Frametime Benchmark ===");
    println!("Query:                {query:?}");
    println!("Corpus / Ingested:    {total_items} items ({matches} matched)");
    println!(
        "Total Frames:         {count} frames across {:.2?}",
        total_elapsed
    );
    println!("Min Frametime:        {min_ms:.3} ms");
    println!("p50 Frametime:        {p50_ms:.3} ms");
    println!("p90 Frametime:        {p90_ms:.3} ms");
    println!("p95 Frametime:        {p95_ms:.3} ms");
    println!("p99 Frametime:        {p99_ms:.3} ms");
    println!("Max Frametime:        {max_ms:.3} ms");

    if p95_ms <= 16.67 {
        println!("Status:               PASS (sub-16.67ms / 60 FPS continuous)");
        0
    } else {
        println!("Status:               WARN (p95 frametime exceeded 16.67ms)");
        0
    }
}
