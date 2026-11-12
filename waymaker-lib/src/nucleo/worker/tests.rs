use super::*;
use nucleo::{Matcher, Nucleo};
use ratatui::style::{Color, Style};
use ratatui::text::Text;
use std::sync::Arc;

/// Plain items store their text once: the matcher's haystack is a slice of the item's own
/// chunk, for the whole line and for each column of a split line.
#[test]
fn test_plain_item_text_is_shared_with_the_matcher() {
    use crate::nucleo::{
        ColumnRanges, Indexed, Segmented, SharedChunk, SharedStr, injector::Injector,
    };
    use crate::utils::Either;
    use nucleo::Utf32String;
    type Item = Indexed<Segmented<Either<SharedStr, Box<Text<'static>>>>>;
    let item = |text: &SharedStr, ranges: ColumnRanges| Indexed {
        index: 0,
        inner: Segmented::new(Either::Left(text.clone()), ranges, None),
    };
    let shares = |h: &Utf32String, text: &SharedStr, want: &str| {
        h.shared()
            .is_some_and(|h| SharedChunk::ptr_eq(&h.chunk(), &text.chunk()) && h.as_str() == want)
    };

    let worker = Worker::<Item>::new_indexable([""], None);
    let text = SharedChunk::new("src/main.rs\nnext").slice(0..11);
    worker
        .injector()
        .push(item(&text, ColumnRanges::single(0, text.len() as u32)))
        .unwrap();
    let snapshot = worker.nucleo.snapshot();
    let haystack = &snapshot.get_item(0).unwrap().matcher_columns[0];
    assert!(shares(haystack, &text, "src/main.rs"));

    let worker = Worker::<Item>::new_indexable(["path", "line"], None);
    let text = SharedStr::from("src/main.rs:12");
    let ranges = ColumnRanges::from(vec![(0, 11), (12, 14)]);
    worker.injector().push(item(&text, ranges)).unwrap();
    let snapshot = worker.nucleo.snapshot();
    let columns = snapshot.get_item(0).unwrap().matcher_columns;
    assert!(shares(&columns[0], &text, "src/main.rs"));
    assert!(shares(&columns[1], &text, "12"));
}

/// A column range that is not on a char boundary is copied (lossily) instead of panicking.
#[test]
fn test_column_off_char_boundary_is_not_shared() {
    use crate::nucleo::{ColumnIndexable, ColumnRanges, Segmented, SharedStr};
    use crate::utils::Either;
    let seg: Segmented<Either<SharedStr, Box<Text<'static>>>> = Segmented::new(
        Either::Left(SharedStr::from("ab\u{e9}c")),
        ColumnRanges::from(vec![(0, 3), (3, 5)]),
        None,
    );
    assert!(seg.get_shared(0).is_none());
    assert!(seg.get_shared(1).is_none());
}

/// Sets up the necessary Nucleo state to trigger a match
fn setup_nucleo_mocks(search_query: &str, item_text: &str) -> (Nucleo<String>, Matcher, Vec<u32>) {
    let mut nucleo = Nucleo::<String>::new(nucleo::Config::DEFAULT, Arc::new(|| {}), None, 1);

    let injector = nucleo.injector();
    injector.push(item_text.to_string(), |item, columns| {
        columns[0] = item.clone().into();
    });

    nucleo.pattern.reparse(
        0,
        search_query,
        nucleo::pattern::CaseMatching::Ignore,
        nucleo::pattern::Normalization::Smart,
        false,
    );

    nucleo.tick(10); // Process the item

    let matcher = Matcher::default();
    let buffer = Vec::new();

    (nucleo, matcher, buffer)
}

#[test]
fn test_no_scroll_context_renders_normally() {
    let (nucleo, mut matcher, mut buffer) = setup_nucleo_mocks("match", "hello match world");
    let snapshot = nucleo.snapshot();
    let item = snapshot.get_item(0).unwrap();

    let cell = Text::from("hello match world");
    let highlight = Style::default().fg(Color::Red);

    let (result_text, width) = render_cell(
        cell,
        0,
        snapshot,
        &item,
        &mut matcher,
        highlight,
        false,
        u16::MAX,
        &mut buffer,
        AutoscrollSettings {
            enabled: false,
            ..Default::default()
        },
        0,
    );

    let output_str = result_text.to_string();
    assert_eq!(output_str, "hello match world");
    assert_eq!(width, 17);
}

#[test]
fn test_scroll_context_cuts_prefix_correctly() {
    let (nucleo, mut matcher, mut buffer) = setup_nucleo_mocks("match", "hello match world");
    let snapshot = nucleo.snapshot();
    let item = snapshot.get_item(0).unwrap();

    let cell = Text::from("hello match world");
    let highlight = Style::default().fg(Color::Red);

    let (result_text, _) = render_cell(
        cell,
        0,
        snapshot,
        &item,
        &mut matcher,
        highlight,
        false,
        u16::MAX,
        &mut buffer,
        AutoscrollSettings {
            initial_preserved: 0,
            context: 2,
            ..Default::default()
        },
        0,
    );

    let output_str = result_text.to_string();
    assert_eq!(output_str, "hello match world");
}

#[test]
fn test_scroll_context_backfills_to_fill_width_limit() {
    // Query "match". Starts at index 10.
    // "abcdefghijmatch"
    // autoscroll = Some((preserved=0, context=1))
    // initial_start_idx = 10 + 0 - 1 = 9 ("jmatch").
    // width_limit = 10.
    // tail_width ("jmatch") = 6.
    // Try to decrease start_idx.
    // start_idx=8 ("ijmatch"), tail_width=7.
    // start_idx=7 ("hijmatch"), tail_width=8.
    // start_idx=6 ("ghijmatch"), tail_width=9.
    // start_idx=5 ("fghijmatch"), tail_width=10.
    // start_idx=4 ("efghijmatch"), tail_width=11 > 10 (STOP).
    // Result start_idx = 5. Output: "fghijmatch"

    let (nucleo, mut matcher, mut buffer) = setup_nucleo_mocks("match", "abcdefghijmatch");
    let snapshot = nucleo.snapshot();
    let item = snapshot.get_item(0).unwrap();

    let cell = Text::from("abcdefghijmatch");
    let highlight = Style::default().fg(Color::Red);

    let (result_text, width) = render_cell(
        cell,
        0,
        snapshot,
        &item,
        &mut matcher,
        highlight,
        false,
        10,
        &mut buffer,
        AutoscrollSettings {
            initial_preserved: 0,
            context: 1,
            indicator: "…".to_string(),
            ..Default::default()
        },
        0,
    );

    let output_str = result_text.to_string();
    assert_eq!(output_str, "…ghijmatch");
    assert_eq!(width, 10);
}

#[test]
fn test_preserved_prefix_and_ellipsis() {
    // Query "match". Starts at index 10.
    // "abcdefghijmatch"
    // autoscroll = Some((preserved=3, context=1))
    // initial_start_idx = 10 + 0 - 1 = 9.
    // start_idx = 9.
    // width_limit = 10.
    // preserved_width ("abc") = 3.
    // gap_indicator_width ("…") = 1.
    // tail_width ("jmatch") = 6.
    // total = 3 + 1 + 6 = 10.
    // start_idx=9, preserved=3. 9 > 3 + 1 (9 > 4) -> preserved_prefix = "abc", output: "abc…jmatch"

    let (nucleo, mut matcher, mut buffer) = setup_nucleo_mocks("match", "abcdefghijmatch");
    let snapshot = nucleo.snapshot();
    let item = snapshot.get_item(0).unwrap();

    let cell = Text::from("abcdefghijmatch");
    let highlight = Style::default().fg(Color::Red);

    let (result_text, width) = render_cell(
        cell,
        0,
        snapshot,
        &item,
        &mut matcher,
        highlight,
        false,
        10,
        &mut buffer,
        AutoscrollSettings {
            initial_preserved: 3,
            context: 1,
            indicator: "…".to_string(),
            ..Default::default()
        },
        0,
    );

    let output_str = result_text.to_string();
    assert_eq!(output_str, "abc…jmatch");
    assert_eq!(width, 10);
}

#[test]
fn test_wrap() {
    let (nucleo, mut matcher, mut buffer) = setup_nucleo_mocks("match", "abcdefmatch");
    let snapshot = nucleo.snapshot();
    let item = snapshot.get_item(0).unwrap();

    let cell = Text::from("abcdefmatch");
    let highlight = Style::default().fg(Color::Red);

    let (result_text, width) = render_cell(
        cell,
        0,
        snapshot,
        &item,
        &mut matcher,
        highlight,
        true,
        10,
        &mut buffer,
        AutoscrollSettings {
            initial_preserved: 3,
            context: 1,
            ..Default::default()
        },
        -2,
    );

    let output_str = result_text.to_string();
    assert_eq!(output_str, "abcdefmat↵\nch");
    assert_eq!(width, 10);
}

#[test]
fn test_wrap_edge_case_6_chars_width_5() {
    let (nucleo, mut matcher, mut buffer) = setup_nucleo_mocks("", "123456");
    let snapshot = nucleo.snapshot();
    let item = snapshot.get_item(0).unwrap();

    let cell = Text::from("123456");
    let highlight = Style::default().fg(Color::Red);

    let (result_text, width) = render_cell(
        cell,
        0,
        snapshot,
        &item,
        &mut matcher,
        highlight,
        true,
        5,
        &mut buffer,
        AutoscrollSettings {
            enabled: false,
            ..Default::default()
        },
        0,
    );

    let output_str = result_text.to_string();
    // Expecting "1234↵" and "56"
    assert_eq!(output_str, "1234↵\n56");
    assert_eq!(width, 5);
}

#[test]
fn test_autoscroll_end() {
    let (nucleo, mut matcher, mut buffer) = setup_nucleo_mocks("match", "abcdefghijmatch");
    let snapshot = nucleo.snapshot();
    let item = snapshot.get_item(0).unwrap();

    let cell = Text::from("abcdefghijmatch");
    let highlight = Style::default().fg(Color::Red);

    let (result_text, width) = render_cell(
        cell,
        0,
        snapshot,
        &item,
        &mut matcher,
        highlight,
        false,
        10,
        &mut buffer,
        AutoscrollSettings {
            end: true,
            context: 4,
            indicator: "…".to_string(),
            ..Default::default()
        },
        0,
    );

    let output_str = result_text.to_string();
    assert_eq!(output_str, "…ghijmatch");
    assert_eq!(width, 10);
}

#[test]
fn test_autoscroll_end_context() {
    let (nucleo, mut matcher, mut buffer) = setup_nucleo_mocks("ma", "abcdefghijmatch");
    let snapshot = nucleo.snapshot();
    let item = snapshot.get_item(0).unwrap();

    let cell = Text::from("abcdefghijmatch");
    let highlight = Style::default().fg(Color::Red);

    let (result_text, width) = render_cell(
        cell,
        0,
        snapshot,
        &item,
        &mut matcher,
        highlight,
        false,
        10,
        &mut buffer,
        AutoscrollSettings {
            end: true,
            context: 2,
            indicator: "…".to_string(),
            ..Default::default()
        },
        0,
    );

    let output_str = result_text.to_string();
    assert_eq!(output_str, "…fghijmatc");
    assert_eq!(width, 10);
}

#[test]
fn test_tier_separators_emitted_on_tier_boundaries() {
    let mut worker = Worker::<String>::new_single_column();
    worker.dir_first = true;

    let injector = worker.nucleo.injector();
    // Tier 0: Direct dirs (ends with /)
    injector.push("alpha/".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    injector.push("beta/".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    // Tier 1: Direct files (no /)
    injector.push("file_a.txt".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    injector.push("file_b.txt".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    // Tier 2: Deep items (has / in middle)
    injector.push("sub/deep_file.txt".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });

    worker.nucleo.tick(10);

    let mut matcher = Matcher::default();
    let (results, _, _, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );

    assert_eq!(results.len(), 5);
    // Item 0: Tier 0 ("alpha/") - first item overall -> None
    assert_eq!(results[0].0, None);
    assert_eq!(results[0].2, "alpha/");

    // Item 1: Tier 0 ("beta/") - same tier -> None
    assert_eq!(results[1].0, None);
    assert_eq!(results[1].2, "beta/");

    // Item 2: Tier 1 ("file_a.txt") - tier boundary 0 -> 1 -> Some(TierSeparator)
    assert_eq!(results[2].0, Some(GroupHeader::TierSeparator));
    assert_eq!(results[2].2, "file_a.txt");

    // Item 3: Tier 1 ("file_b.txt") - same tier -> None
    assert_eq!(results[3].0, None);
    assert_eq!(results[3].2, "file_b.txt");

    // Item 4: Tier 2 ("sub/deep_file.txt") - tier boundary 1 -> 2 -> Some(TierSeparator)
    assert_eq!(results[4].0, Some(GroupHeader::TierSeparator));
    assert_eq!(results[4].2, "sub/deep_file.txt");
}

#[test]
fn test_tier_separators_with_scrolling() {
    let mut worker = Worker::<String>::new_single_column();
    worker.dir_first = true;

    let injector = worker.nucleo.injector();
    injector.push("dir/".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    injector.push("file.txt".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    injector.push("deep/item.txt".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });

    worker.nucleo.tick(10);

    let mut matcher = Matcher::default();
    // Query window starting at index 1 ("file.txt")
    let (results, _, _, _) = worker.results(
        1,
        3,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );

    assert_eq!(results.len(), 2);
    // Item 0 in slice is "file.txt" (Tier 1). Previous item in dataset was "dir/" (Tier 0).
    // Since tier changed from 0 to 1, TierSeparator must be emitted!
    assert_eq!(results[0].0, Some(GroupHeader::TierSeparator));
    assert_eq!(results[0].2, "file.txt");

    // Item 1 in slice is "deep/item.txt" (Tier 2). Previous item was Tier 1.
    // TierSeparator must be emitted!
    assert_eq!(results[1].0, Some(GroupHeader::TierSeparator));
    assert_eq!(results[1].2, "deep/item.txt");
}

#[test]
fn test_tier_separators_disabled_when_dir_first_is_false() {
    let mut worker = Worker::<String>::new_single_column();
    worker.dir_first = false;

    let injector = worker.nucleo.injector();
    injector.push("dir/".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    injector.push("file.txt".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });

    worker.nucleo.tick(10);

    let mut matcher = Matcher::default();
    let (results, _, _, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, None);
    assert_eq!(results[1].0, None);
}

#[test]
fn test_worker_sort_orders() {
    use crate::action::SortOrder;

    let mut worker = Worker::<String>::new_single_column();
    let injector = worker.nucleo.injector();
    injector.push("file10.txt".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    injector.push("file2.txt".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    injector.push("file1.txt".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    injector.push("a.zip".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });
    injector.push("b.tar".to_string(), |item, cols| {
        cols[0] = item.clone().into();
    });

    worker.nucleo.set_stability(u32::MAX);
    worker.nucleo.tick(10);
    let mut matcher = Matcher::default();

    // 1. Alphabetical
    worker.set_sort_order(Some(SortOrder::Alphabetical));
    let (results, _, _, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );
    let items: Vec<&String> = results.iter().map(|r| r.2).collect();
    assert_eq!(
        items,
        vec!["a.zip", "b.tar", "file1.txt", "file10.txt", "file2.txt"]
    );

    // 2. Alphabetical Reverse
    worker.set_sort_order(Some(SortOrder::AlphabeticalReverse));
    let (results, _, _, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );
    let items: Vec<&String> = results.iter().map(|r| r.2).collect();
    assert_eq!(
        items,
        vec!["file2.txt", "file10.txt", "file1.txt", "b.tar", "a.zip"]
    );

    // 3. Natural
    worker.set_sort_order(Some(SortOrder::Natural));
    let (results, _, _, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );
    let items: Vec<&String> = results.iter().map(|r| r.2).collect();
    assert_eq!(
        items,
        vec!["a.zip", "b.tar", "file1.txt", "file2.txt", "file10.txt"]
    );

    // 4. Natural Reverse
    worker.set_sort_order(Some(SortOrder::NaturalReverse));
    let (results, _, _, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );
    let items: Vec<&String> = results.iter().map(|r| r.2).collect();
    assert_eq!(
        items,
        vec!["file10.txt", "file2.txt", "file1.txt", "b.tar", "a.zip"]
    );

    // 5. Extension
    worker.set_sort_order(Some(SortOrder::Extension));
    let (results, _, _, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );
    let items: Vec<&String> = results.iter().map(|r| r.2).collect();
    // .tar < .txt < .zip
    assert_eq!(
        items,
        vec!["b.tar", "file1.txt", "file2.txt", "file10.txt", "a.zip"]
    );

    // 6. Extension Reverse
    worker.set_sort_order(Some(SortOrder::ExtensionReverse));
    let (results, _, _, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );
    let items: Vec<&String> = results.iter().map(|r| r.2).collect();
    // .zip > .txt > .tar
    assert_eq!(
        items,
        vec!["a.zip", "file1.txt", "file2.txt", "file10.txt", "b.tar"]
    );

    // 7. None (preserves exact stream/command insertion order)
    worker.set_sort_order(None);
    let (results, _, _, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );
    let items: Vec<&String> = results.iter().map(|r| r.2).collect();
    assert_eq!(
        items,
        vec!["file10.txt", "file2.txt", "file1.txt", "a.zip", "b.tar"]
    );
}

/// The metadata sorts read the files: sizes and mtimes are set so each order is distinct
/// from the name order (big < mid < small by name).
#[test]
fn test_worker_metadata_sort_orders() {
    use crate::action::SortOrder;
    use std::time::{Duration, SystemTime};

    let dir = std::env::temp_dir().join(format!(
        "wm-sort-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    // (name, size, mtime seconds after the epoch); created in this order.
    let files = [("mid", 50, 3_000), ("small", 1, 1_000), ("big", 900, 2_000)];
    let mut worker = Worker::<String>::new_single_column();
    let injector = worker.nucleo.injector();
    for (name, size, secs) in files {
        let path = dir.join(name);
        std::fs::write(&path, vec![b'x'; size]).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
        std::thread::sleep(Duration::from_millis(5)); // distinct birth times
        injector.push(path.to_string_lossy().into_owned(), |item, cols| {
            cols[0] = item.clone().into();
        });
    }
    // A missing file sorts as the epoch / size 0 instead of being dropped.
    injector.push(
        dir.join("gone").to_string_lossy().into_owned(),
        |item, cols| {
            cols[0] = item.clone().into();
        },
    );
    worker.nucleo.set_stability(u32::MAX);
    worker.nucleo.tick(10);
    let mut matcher = Matcher::default();

    let mut order = |sort| {
        worker.set_sort_order(Some(sort));
        let (results, _, _, _) = worker.results(
            0,
            10,
            &[200],
            false,
            0,
            Style::default(),
            &mut matcher,
            AutoscrollSettings::default(),
            0,
            (0, false),
            true,
            false,
        );
        results
            .iter()
            .map(|r| r.2.rsplit('/').next().unwrap().to_string())
            .collect::<Vec<_>>()
    };

    assert_eq!(order(SortOrder::Modified), ["gone", "small", "big", "mid"]);
    assert_eq!(
        order(SortOrder::ModifiedReverse),
        ["mid", "big", "small", "gone"]
    );
    assert_eq!(order(SortOrder::Size), ["gone", "small", "mid", "big"]);
    assert_eq!(
        order(SortOrder::SizeReverse),
        ["big", "mid", "small", "gone"]
    );
    // Birth time where the filesystem records it, else the mtime: either way "gone" is
    // first and the three files keep one consistent order.
    let created = order(SortOrder::Created);
    assert_eq!(created[0], "gone");
    assert!(
        created[1..] == ["mid", "small", "big"] || created[1..] == ["small", "big", "mid"],
        "{created:?}"
    );
    let mut reverse = order(SortOrder::CreatedReverse);
    reverse.reverse();
    assert_eq!(reverse, created);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_awt_type_ordering() {
    use crate::Matchmaker;
    use crate::config::*;
    use crate::nucleo::injector::Injector;

    let mut columns = ColumnsConfig::default();
    columns.split = Split::Delimiter(regex::Regex::new("\t").unwrap());
    columns.default = Some(StringValue("type".to_string()));
    columns.names = vec![
        ColumnSetting {
            name: ColumnName("type".to_string()),
            hidden: false,
            ignore: false,
            options: ColumnOptions::default(),
        },
        ColumnSetting {
            name: ColumnName("description".to_string()),
            hidden: false,
            ignore: false,
            options: ColumnOptions::default(),
        },
        ColumnSetting {
            name: ColumnName("icon".to_string()),
            hidden: true,
            ignore: false,
            options: ColumnOptions::default(),
        },
        ColumnSetting {
            name: ColumnName("prefix".to_string()),
            hidden: true,
            ignore: false,
            options: ColumnOptions::default(),
        },
    ];

    let mut worker_config = WorkerConfig::default();
    worker_config.sort.threshold = SortThreshold::SMART;

    let (mut mm, injector, _) = Matchmaker::new_from_config(
        RenderConfig::default(),
        TerminalConfig::default(),
        worker_config,
        columns,
        ExitConfig::default(),
        (false, false),
    );

    let lines = vec![
        "\u{1b}[1;36m feat\u{1b}[0m\tNew feature\t\tfeat/\n",
        "\u{1b}[1;31m fix\u{1b}[0m\tBug fix\t\tfix/\n",
        "\u{1b}[1;33m󰣪 refactor\u{1b}[0m\tCode restructuring\t󰣪\trefactor/\n",
        "\u{1b}[1;35m󰓅 perf\u{1b}[0m\tPerformance optimization\t󰓅\tperf/\n",
        "\u{1b}[1;36m ci\u{1b}[0m\tCI/CD workflows / pipelines\t\tci/\n",
        "\u{1b}[1;34m chore\u{1b}[0m\tMaintenance / config\t\tchore/\n",
        "\u{1b}[1;32m󰧮 docs\u{1b}[0m\tDocumentation\t󰧮\tdocs/\n",
        "\u{1b}[1;36m󰙨 test\u{1b}[0m\tTest suites\t󰙨\ttest/\n",
        "\u{1b}[1;33m󰏖 build\u{1b}[0m\tDependencies / build\t󰏖\tbuild/\n",
        "\u{1b}[2m󰓹 custom\u{1b}[0m\tNo prefix / freeform\t󰓹\t\n",
    ];

    for line in &lines {
        let _ = injector.push((None, (*line).into()));
    }

    mm.worker.nucleo.tick(10);
    let mut matcher = Matcher::default();

    let (results, _, _, _) = mm.worker.results(
        0,
        10,
        &[100, 100, 100, 100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );

    let items: Vec<String> = results
        .iter()
        .map(|r| {
            // Return column 0 raw or text
            r.1[0].to_string()
        })
        .collect();
    println!("Items returned:\n{:#?}", items);

    assert_eq!(items[0], "\u{1b}[1;36m feat\u{1b}[0m");
    assert_eq!(items[1], "\u{1b}[1;31m fix\u{1b}[0m");
    assert_eq!(items[2], "\u{1b}[1;33m󰣪 refactor\u{1b}[0m");
    assert_eq!(items[3], "\u{1b}[1;35m󰓅 perf\u{1b}[0m");
    assert_eq!(items[4], "\u{1b}[1;36m ci\u{1b}[0m");
    assert_eq!(items[5], "\u{1b}[1;34m chore\u{1b}[0m");
    assert_eq!(items[6], "\u{1b}[1;32m󰧮 docs\u{1b}[0m");
    assert_eq!(items[7], "\u{1b}[1;36m󰙨 test\u{1b}[0m");
    assert_eq!(items[8], "\u{1b}[1;33m󰏖 build\u{1b}[0m");
    assert_eq!(items[9], "\u{1b}[2m󰓹 custom\u{1b}[0m");
}

#[test]
fn test_frecency_mode_preserves_order_and_no_tier_separators() {
    let mut worker = Worker::<String>::new_single_column();
    worker.dir_first = true;
    worker.depth_penalty = 15;
    worker.set_mode_index(1);
    worker.set_sort_order(Some(crate::action::SortOrder::ModifiedReverse));
    worker.set_stability(crate::config::SortThreshold::SMART);

    let paths = vec![
        "/home/fecavmi/Downloads".to_string(),
        "/home/fecavmi/.local/state/omarchy/current/theme/backgrounds".to_string(),
        "/home/fecavmi/.dotfiles/main".to_string(),
        "/home/fecavmi/dev/github/matchmaker/feat-bookmarks".to_string(),
        "/home/fecavmi/.dotfiles/main/.agents".to_string(),
        "/".to_string(),
        "/home/fecavmi".to_string(),
        "/tmp".to_string(),
    ];

    let injector = worker.nucleo.injector();
    for p in &paths {
        injector.push(p.clone(), |item, cols| {
            cols[0] = item.clone().into();
        });
    }

    while worker.nucleo.snapshot().item_count() < paths.len() as u32 {
        worker.nucleo.tick(10);
    }

    for (idx, expected) in paths.iter().enumerate() {
        let nth = worker.get_nth(idx as u32);
        assert_eq!(nth, Some(expected), "get_nth mismatch at index {idx}");
    }

    let mut matcher = Matcher::default();
    let (results, _, _, _) = worker.results(
        0,
        paths.len() as u32,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );

    for (idx, (header, _, item_data)) in results.iter().enumerate() {
        eprintln!("{idx}: {item_data} (header={header:?})");
    }
    for (idx, (header, _, item_data)) in results.iter().enumerate() {
        assert_eq!(*header, None, "Expected no tier separator in mode 1");
        assert_eq!(*item_data, &paths[idx], "Mismatch at position {idx}");
    }
}

#[test]
fn test_find_item_index() {
    let mut worker = Worker::<String>::new_single_column();
    worker.dir_first = true;
    worker.depth_penalty = 15;

    let items = vec![
        "src/sub/deep.rs".to_string(),
        "docs/".to_string(),
        "Cargo.toml".to_string(),
        "src/".to_string(),
        "README.md".to_string(),
    ];

    let injector = worker.nucleo.injector();
    for item in &items {
        injector.push(item.clone(), |val, cols| {
            cols[0] = val.clone().into();
        });
    }

    while worker.nucleo.snapshot().item_count() < items.len() as u32 {
        worker.nucleo.tick(10);
    }

    // With dir_first = true:
    // Tier 0 (direct dirs): docs/, src/ (alphabetical: docs/, src/)
    // Tier 1 (direct files): Cargo.toml, README.md (alphabetical)
    // Tier 2 (deep items): src/sub/deep.rs
    assert_eq!(worker.find_item_index(|s| s == "docs/"), Some(0));
    assert_eq!(worker.find_item_index(|s| s == "src/"), Some(1));
    assert_eq!(worker.find_item_index(|s| s == "Cargo.toml"), Some(2));
    assert_eq!(worker.find_item_index(|s| s == "README.md"), Some(3));
    assert_eq!(worker.find_item_index(|s| s == "src/sub/deep.rs"), Some(4));
    assert_eq!(worker.find_item_index(|s| s == "nonexistent"), None);

    // Verify get_nth matches find_item_index
    for target in &items {
        let idx = worker.find_item_index(|s| s == target).unwrap();
        let nth = worker.get_nth(idx as u32).unwrap();
        assert_eq!(nth, target);
    }

    // Verify get_all_sorted returns the exact ranked list
    let all_sorted: Vec<&String> = worker.get_all_sorted();
    assert_eq!(
        all_sorted,
        vec![
            "docs/",
            "src/",
            "Cargo.toml",
            "README.md",
            "src/sub/deep.rs"
        ]
    );
}

#[test]
fn test_active_query_ranks_by_relevance_within_tier_over_alphabetical() {
    let mut worker = Worker::<String>::new_single_column();
    worker.dir_first = true;

    let items = vec![
        "CHANGELOG.md".to_string(),
        "oi.md".to_string(),
        "HELLO.md".to_string(),
    ];

    let injector = worker.nucleo.injector();
    for item in &items {
        injector.push(item.clone(), |val, cols| {
            cols[0] = val.clone().into();
        });
    }

    while worker.nucleo.snapshot().item_count() < items.len() as u32 {
        worker.nucleo.tick(10);
    }

    // When query is empty: Tier 1 items are sorted alphabetically
    let all_empty = worker.get_all_sorted();
    assert_eq!(all_empty, vec!["CHANGELOG.md", "HELLO.md", "oi.md"]);

    // When query is "oi.md": exact match oi.md must rank 1st
    worker.find("oi.md");
    while worker.nucleo.tick(10).running {}

    let all_filtered = worker.get_all_sorted();
    assert_eq!(*all_filtered[0], "oi.md");
}

/// A line wider than `u16::MAX` cells measures as wide, not as its width modulo 65,536: the
/// medians that lay out the columns saw a 65,546-cell line as 10 cells and cut its row short.
#[test]
fn test_width_of_a_line_wider_than_u16_saturates() {
    let mut worker = Worker::<String>::new_single_column();
    let injector = worker.nucleo.injector();
    let long = "x".repeat(65_546);
    for line in [long.clone(), long.clone(), long, "short".to_string()] {
        injector.push(line, |item, cols| {
            cols[0] = item.clone().into();
        });
    }
    worker.nucleo.tick(10);

    let mut matcher = Matcher::default();
    let (results, _, medians, _) = worker.results(
        0,
        10,
        &[100],
        false,
        0,
        Style::default(),
        &mut matcher,
        AutoscrollSettings::default(),
        0,
        (0, false),
        true,
        false,
    );
    assert_eq!(results.len(), 4);
    assert_eq!(medians[0], crate::utils::MAX_CELLS, "medians: {medians:?}");
}

/// `all_items` ignores the query: leaving a directory caches its listing from it, and the query
/// is cleared on the way back, so caching only the matches hid the rest of the directory.
#[test]
fn test_all_items_includes_unmatched_items() {
    let mut worker = Worker::<String>::new_single_column();
    let injector = worker.nucleo.injector();
    for item in ["alpha", "beta", "gamma"] {
        injector.push(item.to_string(), |val, cols| {
            cols[0] = val.clone().into();
        });
    }
    worker.find("alpha");
    while worker.nucleo.tick(10).running || worker.nucleo.snapshot().item_count() < 3 {}

    assert_eq!(worker.raw_results().collect::<Vec<_>>(), ["alpha"]);
    assert_eq!(
        worker.all_items().collect::<Vec<_>>(),
        ["alpha", "beta", "gamma"]
    );
}
