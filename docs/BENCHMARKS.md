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

Measured on Linux `aarch64` (release build, median of 3 runs, `stdin` redirected from file, `stdout` to `/dev/null`):

| Query Pattern | Query | Matches | `wm -f` | `fzf -f` | `sk -f` |
| :--- | :--- | ---: | ---: | ---: | ---: |
| Narrow | `mod97file12` | 27,305 | **199 ms** | 202 ms | 613 ms |
| Broad | `src/module_5` | 1,559,075 | **424 ms** | 1,128 ms | 2,521 ms |
| No match | `xyzzy` | 0 | **189 ms** | 171 ms | 241 ms |
| Peak RSS (narrow) | `mod97file12` | — | **321 MiB** | 244 MiB | 296 MiB |
| Peak RSS (broad) | `src/module_5` | — | **373 MiB** | 544 MiB | — |

CPU time (user) on the same corpus, 8 cores: `wm -f` uses about 1.3 s on narrow and no-match queries against `fzf`'s 0.2–0.6 s, and 2.3 s against 3.9 s on the broad one. Pinned to one core it is 0.42 s against 0.52 s, so the difference is the parallel matcher's overhead per thread (nucleo's rayon pool), not per-item work.

## Interactive Streaming Frametime (`300,000` items)

| Percentile | Frametime | Budget (60 FPS) |
| :--- | ---: | ---: |
| p50 | 0.17 ms | 16.67 ms |
| p90 | 2.70 ms | 16.67 ms |
| p99 / Max | 7.60 ms | 16.67 ms |
