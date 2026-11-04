use super::*;

/// Splits `reader` into records ending in `delim` and passes each one to `f`, without the
/// delimiter (and without a `\r` before it when `delim` is `\n`). A last record with no
/// trailing delimiter is passed too.
///
/// Invalid UTF-8 is replaced with U+FFFD, like fzf, instead of dropping the record: one
/// non-UTF-8 file name must not hide it, or (as `read_line`'s error did in the headless
/// filter) the whole input. Records are found with `memchr` in a large buffer and each
/// costs one allocation.
pub fn for_each_record<E>(
    reader: impl Read,
    delim: u8,
    mut f: impl FnMut(String) -> Result<(), E>,
) -> Result<usize, MapReaderError<E>> {
    use std::io::BufRead;

    let mut reader = std::io::BufReader::with_capacity(256 * 1024, reader);
    // A record split across two buffer fills.
    let mut partial: Vec<u8> = Vec::new();
    let mut count = 0;
    let mut lossy = 0usize;

    let mut emit = |mut record: &[u8], count: &mut usize| -> Result<(), MapReaderError<E>> {
        if delim == b'\n'
            && let Some(rest) = record.strip_suffix(b"\r")
        {
            record = rest;
        }
        let text = match std::str::from_utf8(record) {
            Ok(s) => s.to_owned(),
            Err(_) => {
                lossy += 1;
                String::from_utf8_lossy(record).into_owned()
            }
        };
        f(text).map_err(MapReaderError::Custom)?;
        *count += 1;
        Ok(())
    };

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
        for end in memchr::memchr_iter(delim, buf) {
            if partial.is_empty() {
                emit(&buf[start..end], &mut count)?;
            } else {
                partial.extend_from_slice(&buf[start..end]);
                emit(&partial, &mut count)?;
                partial.clear();
            }
            start = end + 1;
        }
        partial.extend_from_slice(&buf[start..]);
        let len = buf.len();
        reader.consume(len);
    }
    if !partial.is_empty() {
        emit(&partial, &mut count)?;
    }
    drop(emit);
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
    f: impl FnMut(String) -> Result<(), E> + SSS,
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

/// Spawns a tokio task reading records (see [`for_each_record`]) and batching 512 items into `injector.extend()`.
pub fn map_reader_batched(
    reader: impl Read + Send + 'static,
    injector: ConfigInjector,
    group_prefix: Option<String>,
    input_separator: Option<char>,
    abort_empty: Option<RenderSender<MMAction>>,
) -> tokio::task::JoinHandle<Result<usize, anyhow::Error>> {
    tokio::task::spawn_blocking(move || {
        let mut total = 0;
        let mut batch: Vec<(Option<std::sync::Arc<str>>, String)> = Vec::with_capacity(512);
        let mut current_group: Option<std::sync::Arc<str>> = None;

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
                current_group = Some(group.trim().into());
                return Ok(());
            }
            total += 1;
            batch.push((current_group.clone(), line));
            if batch.len() >= 512 {
                let chunk = std::mem::replace(&mut batch, Vec::with_capacity(512));
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
) -> impl FnMut(String) -> Result<(), waymaker::nucleo::WorkerError> + Send {
    let mut header_buf = Vec::with_capacity(header_lines);
    let mut remaining = header_lines;
    let injector = injector;
    let mut current_group: Option<std::sync::Arc<str>> = None;

    // For each row, take the first line of each segmented column, building a Vec<Vec<Line>>
    move |line: String| {
        if let Some(prefix) = &group_prefix
            && line.starts_with(prefix)
        {
            current_group = Some(line.strip_prefix(prefix).unwrap().trim().into());
            return Ok(());
        }

        if remaining > 0 {
            let item = injector.wrap((current_group.clone(), line)).unwrap();
            let item = injector.injector.wrap(item).unwrap();
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
