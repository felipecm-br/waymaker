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
| Peak RSS (narrow) | `mod97file12` | — | **357 MiB** | 244 MiB | 296 MiB |
| Peak RSS (broad) | `src/module_5` | — | **401 MiB** | 534 MiB | — |

## Interactive Streaming Frametime (`300,000` items)

| Percentile | Frametime | Budget (60 FPS) |
| :--- | ---: | ---: |
| p50 | 0.17 ms | 16.67 ms |
| p90 | 2.70 ms | 16.67 ms |
| p99 / Max | 7.60 ms | 16.67 ms |
