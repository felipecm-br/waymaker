# Waymaker Benchmarks

Reproducible headless filter (`wm -f`), interactive streaming frametime (`--benchmark-input`), and Criterion matcher benchmarks.

## Running Benchmarks

```sh
# 1. Headless filter comparison across 10k and 100k synthetic paths
just bench-filter --markdown

# 2. 2,000,000-line (113 MB) audit corpus with median latency + Peak RSS
just bench-filter --audit --markdown

# 3. Interactive TUI streaming frametime (p50 / p90 / p95 / p99)
seq 1 300000 | awk '{print "src/module_" $1 % 977 "/file_" $1 ".rs"}' \
  | wm --config waymaker-cli/assets/config.toml --benchmark-input mod97

# 4. Criterion microbenchmarks for Nucleo & Frizbee matching engines
just bench
```

## Headless Filter (`2,000,000` lines, 113 MB)

Measured on Linux `aarch64` (release build, median of 11 interleaved runs, `stdin` redirected from file, `stdout` to `/dev/null`; `fzf` 0.74.3):

| Query Pattern | Query | Matches | `wm -f` | `fzf -f` | `sk -f` |
| :--- | :--- | ---: | ---: | ---: | ---: |
| Narrow | `mod97file12` | 27,305 | **87 ms** | 195 ms | 613 ms |
| Broad | `src/module_5` | 1,559,075 | **420 ms** | 1,149 ms | 2,521 ms |
| No match | `xyzzy` | 0 | **76 ms** | 145 ms | 241 ms |
| Peak RSS (narrow) | `mod97file12` | — | 275 MiB | **244 MiB** | 296 MiB |
| Peak RSS (broad) | `src/module_5` | — | **323 MiB** | 530 MiB | — |

`sk` figures are from an earlier run. CPU time (user) on 8 cores: `wm -f` uses about 0.35 s on the narrow and no-match queries (`fzf`: 0.25–0.55 s) and 2.3 s on the broad one (`fzf`: 3.9 s). Until nucleo `041465e`, every worker thread bumped one shared counter per unmatched item, which cost about 0.9 s of CPU on these queries.

Input lines are slices of one shared chunk per 256 KiB read (`SharedStr`), so a line costs its bytes plus a 16-byte handle in the item and one in the matcher, instead of an 80-byte allocation each (narrow RSS 318 → 275 MiB). The chunk records once that it is plain ASCII, so the injector no longer scans every line again (narrow 120 → 87 ms, no match 111 → 76 ms). The broad median is within run-to-run noise of the old 386–440 ms; in 21 interleaved runs it was 392–406 ms against 419–442 ms before.

## Built-in Walker (no piped input)

With stdin on a terminal, both tools list the working directory themselves. In `~/.cargo/registry` (84k entries), median of 5 runs, `wm` 0.9.0 against `fzf` 0.74.3:

| Query | `wm -f` | `fzf -f` |
| :--- | ---: | ---: |
| `zzzqqq` (no match) | **34 ms** | 86 ms |
| `mod.rs` | **37 ms** | 100 ms |
| `src` | **65 ms** | 177 ms |

The walkers' defaults differ (directories, hidden files), so the match counts differ slightly (`src`: 80,324 against 68,380).

## Interactive Streaming Frametime (`300,000` items)

| Percentile | Frametime | Budget (60 FPS) |
| :--- | ---: | ---: |
| p50 | 0.17 ms | 16.67 ms |
| p90 | 2.70 ms | 16.67 ms |
| p99 / Max | 7.60 ms | 16.67 ms |
