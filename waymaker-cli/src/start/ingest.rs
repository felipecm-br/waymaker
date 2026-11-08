use super::*;
use std::sync::Arc;
use waymaker::{cache::DirCacheStore, config::WalkerConfig, nucleo::WorkerError};

/// Splits `reader` into records ending in `delim` and passes each one to `f`, without the
/// delimiter (and without a `\r` before it when `delim` is `\n`). A last record with no
/// trailing delimiter is passed too.
///
/// Invalid UTF-8 is replaced with U+FFFD, like fzf, instead of dropping the record: one
/// non-UTF-8 file name must not hide it, or (as `read_line`'s error did in the headless
/// filter) the whole input. Records are found with `memchr` in a large buffer and each
/// costs one allocation: the `Arc<str>` is the text the item and the matcher both keep.
pub fn for_each_record<E>(
    reader: impl Read,
    delim: u8,
    mut f: impl FnMut(Arc<str>) -> Result<(), E>,
) -> Result<usize, MapReaderError<E>> {
    use std::io::BufRead;

    let mut reader = std::io::BufReader::with_capacity(256 * 1024, reader);
    // A record split across two buffer fills.
    let mut partial: Vec<u8> = Vec::new();
    let mut count = 0;
    let mut lossy = 0usize;

    fn emit_record<E>(
        mut record: &[u8],
        delim: u8,
        f: &mut impl FnMut(Arc<str>) -> Result<(), E>,
        count: &mut usize,
        lossy: &mut usize,
    ) -> Result<(), MapReaderError<E>> {
        if delim == b'\n'
            && let Some(rest) = record.strip_suffix(b"\r")
        {
            record = rest;
        }
        let text: Arc<str> = match std::str::from_utf8(record) {
            Ok(s) => s.into(),
            Err(_) => {
                *lossy += 1;
                String::from_utf8_lossy(record).as_ref().into()
            }
        };
        f(text).map_err(MapReaderError::Custom)?;
        *count += 1;
        Ok(())
    }

    loop {
        let buf = match reader.fill_buf() {
            Ok(buf) => buf,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(MapReaderError::ChunkError(count, e)),
        };
        if buf.is_empty() {
            break;
        }
        let mut start = 0;
        if !partial.is_empty() {
            if let Some(end) = memchr::memchr(delim, buf) {
                partial.extend_from_slice(&buf[..end]);
                emit_record(&partial, delim, &mut f, &mut count, &mut lossy)?;
                partial.clear();
                start = end + 1;
            } else {
                partial.extend_from_slice(buf);
                let len = buf.len();
                reader.consume(len);
                continue;
            }
        }
        if let Some(last_rel_end) = memchr::memrchr(delim, &buf[start..]) {
            let last_end = start + last_rel_end;
            if let Ok(valid) = std::str::from_utf8(&buf[start..last_end]) {
                let valid_bytes = valid.as_bytes();
                let mut rel_start = 0;
                for rel_end in memchr::memchr_iter(delim, valid_bytes) {
                    let mut line_end = rel_end;
                    if delim == b'\n' && line_end > rel_start && valid_bytes[line_end - 1] == b'\r'
                    {
                        line_end -= 1;
                    }
                    f(valid[rel_start..line_end].into()).map_err(MapReaderError::Custom)?;
                    count += 1;
                    rel_start = rel_end + 1;
                }
                let mut line_end = valid.len();
                if delim == b'\n' && line_end > rel_start && valid_bytes[line_end - 1] == b'\r' {
                    line_end -= 1;
                }
                f(valid[rel_start..line_end].into()).map_err(MapReaderError::Custom)?;
                count += 1;
                start = last_end + 1;
            } else {
                let chunk_start = start;
                for rel_end in memchr::memchr_iter(delim, &buf[chunk_start..=last_end]) {
                    let abs_end = chunk_start + rel_end;
                    emit_record(&buf[start..abs_end], delim, &mut f, &mut count, &mut lossy)?;
                    start = abs_end + 1;
                }
            }
        }
        partial.extend_from_slice(&buf[start..]);
        let len = buf.len();
        reader.consume(len);
    }
    if !partial.is_empty() {
        emit_record(&partial, delim, &mut f, &mut count, &mut lossy)?;
    }
    if lossy > 0 {
        log::warn!("{lossy} input records were not valid UTF-8; invalid bytes shown as U+FFFD");
    }
    Ok(count)
}

/// The byte input records end with: `input_separator`, or a newline.
pub(super) fn record_delimiter(input_separator: Option<char>) -> u8 {
    match input_separator {
        Some(c) if c.is_ascii() => c as u8,
        Some(c) => {
            log::warn!("Input separator {c:?} is not ASCII; splitting input on newlines");
            b'\n'
        }
        None => b'\n',
    }
}

/// Spawns a tokio task mapping f to reader segments.
/// Read aborts on error. Read errors are logged.
pub fn map_reader<E: SSS + std::fmt::Display>(
    reader: impl Read + SSS,
    f: impl FnMut(Arc<str>) -> Result<(), E> + SSS,
    input_separator: Option<char>,
    abort_empty: Option<RenderSender<MMAction>>,
) -> tokio::task::JoinHandle<Result<usize, MapReaderError<E>>> {
    tokio::task::spawn_blocking(move || {
        let ret = for_each_record(reader, record_delimiter(input_separator), f).elog();

        if let Some(render_tx) = abort_empty
            && matches!(ret, Ok(0))
        {
            let _ = render_tx.send(waymaker::message::RenderCommand::NoMatch);
        }
        log::trace!("All items pushed");
        ret
    })
}

/// Spawns a tokio task reading records (see [`for_each_record`]) and batching 4,096 items into `injector.extend()`.
pub fn map_reader_batched(
    reader: impl Read + Send + 'static,
    injector: ConfigInjector,
    group_prefix: Option<String>,
    input_separator: Option<char>,
    abort_empty: Option<RenderSender<MMAction>>,
) -> tokio::task::JoinHandle<Result<usize, anyhow::Error>> {
    const BATCH_SIZE: usize = 4096;
    tokio::task::spawn_blocking(move || {
        let mut total = 0;
        let mut batch: Vec<(Option<waymaker::nucleo::GroupLabel>, Arc<str>)> =
            Vec::with_capacity(BATCH_SIZE);
        let mut current_group: Option<waymaker::nucleo::GroupLabel> = None;

        // Inject on a separate thread so reading the next batch overlaps with
        // indexing the previous one. A single consumer keeps the input order.
        let (batch_tx, batch_rx) = std::sync::mpsc::sync_channel::<Vec<_>>(16);
        let consumer = std::thread::spawn(move || {
            for chunk in batch_rx {
                if injector.extend(chunk.into_iter()).is_err() {
                    break;
                }
            }
        });

        // Err(()) stops reading: the consumer is gone (the picker shut the injector down).
        let read = for_each_record::<()>(reader, record_delimiter(input_separator), |line| {
            if let Some(prefix) = &group_prefix
                && let Some(group) = line.strip_prefix(prefix.as_str())
            {
                current_group = Some(std::sync::Arc::new(group.trim().into()));
                return Ok(());
            }
            total += 1;
            batch.push((current_group.clone(), line));
            if batch.len() >= BATCH_SIZE {
                let chunk = std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE));
                batch_tx.send(chunk).map_err(|_| ())?;
            }
            Ok(())
        });

        // Inject what was read even when reading failed part-way.
        if !batch.is_empty() {
            let _ = batch_tx.send(batch);
        }
        // Everything is injected once the consumer drains the channel.
        drop(batch_tx);
        let _ = consumer.join();

        if let Err(MapReaderError::ChunkError(i, e)) = read {
            log::error!("Failed to read input after {i} records: {e}");
        }

        if let Some(render_tx) = abort_empty
            && total == 0
        {
            let _ = render_tx.send(waymaker::message::RenderCommand::NoMatch);
        }

        log::trace!("All {total} batched items pushed");
        Ok(total)
    })
}

use waymaker::nucleo::{Line, Span};

pub(super) fn inject_line(
    header_lines: usize,
    render_tx: RenderSender<MMAction>,
    injector: ConfigInjector,
    group_prefix: Option<String>,
) -> impl FnMut(Arc<str>) -> Result<(), waymaker::nucleo::WorkerError> + Send {
    let mut header_buf = Vec::with_capacity(header_lines);
    let mut remaining = header_lines;
    let injector = injector;
    let mut current_group: Option<waymaker::nucleo::GroupLabel> = None;

    // For each row, take the first line of each segmented column, building a Vec<Vec<Line>>
    move |line: Arc<str>| {
        if let Some(prefix) = &group_prefix
            && line.starts_with(prefix)
        {
            current_group = Some(std::sync::Arc::new(
                line.strip_prefix(prefix).unwrap().trim().into(),
            ));
            return Ok(());
        }

        if remaining > 0 {
            let item = injector.wrap((current_group.clone(), line))?;
            let item = injector.injector.wrap(item)?;
            header_buf.push(item);
            remaining -= 1;

            if remaining == 0 {
                let rows: Vec<Vec<Line>> = header_buf
                    .drain(..)
                    .map(|seg| {
                        let row = (0..seg.len())
                            .map(move |i| {
                                let mut s = seg.get_text(i);
                                if s.lines.is_empty() {
                                    Line::default()
                                } else {
                                    to_static(s.lines.remove(0))
                                }
                            })
                            .collect();
                        trim_trailing_empty(row)
                    })
                    .collect();

                let _ = render_tx.send(waymaker::message::RenderCommand::HeaderTable(rows));
            }

            Ok(())
        } else {
            injector.push((current_group.clone(), line))
        }
    }
}

pub(super) fn trim_trailing_empty(mut row: Vec<Line>) -> Vec<Line> {
    while matches!(row.last(), Some(line) if line.iter().all(|x| x.content.is_empty())) {
        row.pop();
    }

    row
}

pub(super) fn to_static(line: Line<'_>) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| {
                Span::styled(
                    span.content.into_owned(), // force ownership
                    span.style,
                )
            })
            .collect::<Vec<_>>(),
    )
}

/// Lists the current directory with the built-in walker (no input and no start command). A
/// valid cached listing is injected at once and refreshed in the background; otherwise items
/// stream in as they are found. Either way the fresh listing is cached for the next run.
pub(super) fn ingest_walk(
    push_fn: impl FnMut(Arc<str>) -> Result<(), WorkerError> + Send + 'static,
    walker_config: &WalkerConfig,
) -> tokio::task::JoinHandle<Result<usize, MapReaderError<WorkerError>>> {
    let cwd_str = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let cache_store = waymaker::cache::DirCacheStore::open();

    let ignored_set: std::collections::HashSet<String> =
        walker_config.effective_ignore().into_iter().collect();
    let is_ignored_item = |item: &str| -> bool {
        let s = item.strip_prefix("./").unwrap_or(item);
        let s = s.strip_prefix('/').unwrap_or(s);
        let first = s.split(['/', '\\']).next().unwrap_or("");
        let clean_first = first.trim_end_matches(['/', '\\']);
        !clean_first.is_empty() && ignored_set.contains(clean_first)
    };

    if let Some(cached_rec) = cache_store.get_valid(&cwd_str)
        && !cached_rec.items.is_empty()
    {
        let mut push_fn = push_fn;
        for item in cached_rec.items {
            if is_ignored_item(&item) {
                continue;
            }
            if push_fn(item.into()).is_err() {
                break;
            }
        }

        let walker_opts = walker_config.to_options(".");
        tokio::spawn(async move {
            let (collect_tx, collect_rx) = std::sync::mpsc::channel();
            let walker = waymaker::walker::AsyncWalker::new(walker_opts);
            let handle = walker.spawn_walk(move |line| {
                let _ = collect_tx.send(line);
                Ok(())
            });
            let _ = handle.await;

            cache_listing(&cache_store, &cwd_str, collect_rx.into_iter().collect());
            Ok(0)
        })
    } else {
        let walker_opts = walker_config.to_options(".");
        tokio::spawn(async move {
            let (collect_tx, collect_rx) = std::sync::mpsc::channel();
            let walker = waymaker::walker::AsyncWalker::new(walker_opts);
            let mut push_fn = push_fn;

            let handle = walker.spawn_walk(move |line| {
                let _ = collect_tx.send(line.clone());
                push_fn(line.into())
            });
            let _ = handle.await;

            cache_listing(&cache_store, &cwd_str, collect_rx.into_iter().collect());
            Ok(0)
        })
    }
}

/// Stores a directory listing, shallow paths first, for [`ingest_walk`] to start from.
fn cache_listing(cache_store: &DirCacheStore, cwd: &str, mut items: Vec<String>) {
    if !items.is_empty() {
        items.sort_by_key(|item| {
            let slashes = item.bytes().filter(|&b| b == b'/' || b == b'\\').count();
            (slashes, item.clone())
        });
        let _ = cache_store.put(cwd, items);
    }
}
