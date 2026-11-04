#!/usr/bin/env python3
"""
Headless Filter Benchmark Harness: wm -f vs fzf -f
Measures latency, throughput, and memory consumption across synthetic corpora.
"""

import argparse
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time


def generate_corpus(size: int, output_path: str):
    """Generate a deterministic synthetic corpus of file paths."""
    modules = [
        "src", "core", "cli", "lib", "engine", "render", "ui", "parser",
        "router", "auth", "database", "network", "storage", "crypto",
        "utils", "config", "plugin", "worker"
    ]
    submodules = [
        "state", "event", "handler", "types", "constants", "view",
        "model", "controller", "service", "client", "stream", "cache",
        "metrics", "builder", "session"
    ]
    extensions = ["rs", "ts", "tsx", "js", "json", "toml", "md", "yaml", "html", "css"]

    root_files = [
        "README.md", "Cargo.toml", "Cargo.lock", "LICENSE-MIT", "LICENSE-APACHE",
        "Makefile", ".gitignore", ".editorconfig", "package.json", "tsconfig.json",
        "docker-compose.yml"
    ]

    with open(output_path, "w", encoding="utf-8") as f:
        for rf in root_files:
            f.write(rf + "\n")

        count = len(root_files)
        i = 0
        while count < size:
            mod_idx = i % len(modules)
            sub_idx = (i // len(modules)) % len(submodules)
            ext_idx = (i // (len(modules) * len(submodules))) % len(extensions)
            depth = (i % 6) + 1

            if depth == 1:
                if i % 3 == 0:
                    path = f"{modules[mod_idx]}/"
                else:
                    path = f"{modules[mod_idx]}_{i}.{extensions[ext_idx]}"
            elif depth == 2:
                path = f"{modules[mod_idx]}/{submodules[sub_idx]}_{i}.{extensions[ext_idx]}"
            elif depth == 3:
                nxt = modules[(mod_idx + 1) % len(modules)]
                path = f"{modules[mod_idx]}/{submodules[sub_idx]}/{nxt}_{i}.{extensions[ext_idx]}"
            elif depth == 4:
                path = f"crates/{modules[mod_idx]}/{submodules[sub_idx]}/src/component_{i}.{extensions[ext_idx]}"
            elif depth == 5:
                nxt = modules[(mod_idx + 2) % len(modules)]
                path = f"crates/{modules[mod_idx]}/{submodules[sub_idx]}/src/internal/{nxt}_{i}.{extensions[ext_idx]}"
            else:
                nxt = modules[(mod_idx + 3) % len(modules)]
                path = f"vendor/{modules[mod_idx]}/packages/{submodules[sub_idx]}/{nxt}/deep/node_{i}.{extensions[ext_idx]}"

            f.write(path + "\n")
            count += 1
            i += 1


def generate_audit_corpus(size: int, output_path: str):
    """Generate the deterministic 2M-line audit corpus matching docs/AUDIT-2026-11.md."""
    with open(output_path, "w", encoding="utf-8") as f:
        for i in range(1, size + 1):
            f.write(f"src/module_{i % 977}/file_{i}.rs path/to/something/{i * 7}\n")


def run_benchmark_command(cmd: list[str], corpus_file: str, runs: int) -> dict:
    """Run a command multiple times with stdin redirected from corpus_file and collect execution + RSS stats."""
    durations = []
    peak_rss_mib = 0.0
    output_lines = 0
    rss_divisor = 1024.0 * 1024.0 if sys.platform == "darwin" else 1024.0

    for run_idx in range(runs):
        with open(corpus_file, "rb") as inp:
            if run_idx == 0:
                proc = subprocess.run(
                    cmd,
                    stdin=inp,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.DEVNULL,
                    check=False,
                )
                if proc.returncode in (0, 1):
                    output_lines = proc.stdout.count(b"\n")
                inp.seek(0)

            start = time.perf_counter()
            if hasattr(os, "wait4"):
                proc = subprocess.Popen(
                    cmd,
                    stdin=inp,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
                _, _, rusage = os.wait4(proc.pid, 0)
                elapsed = time.perf_counter() - start
                peak_rss_mib = max(peak_rss_mib, rusage.ru_maxrss / rss_divisor)
            else:
                subprocess.run(
                    cmd,
                    stdin=inp,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False,
                )
                elapsed = time.perf_counter() - start
            durations.append(elapsed * 1000.0)

    return {
        "median_ms": statistics.median(durations),
        "mean_ms": statistics.mean(durations),
        "stddev_ms": statistics.stdev(durations) if len(durations) > 1 else 0.0,
        "min_ms": min(durations),
        "max_ms": max(durations),
        "rss_mib": peak_rss_mib,
        "lines": output_lines,
    }


def main():
    parser = argparse.ArgumentParser(description="Waymaker Headless Filter Benchmark Harness")
    parser.add_argument("--sizes", nargs="+", type=int, default=[10000, 100000],
                        help="Corpus sizes to benchmark (default: 10000 100000)")
    parser.add_argument("--audit", action="store_true",
                        help="Run the 2,000,000-line audit corpus and queries from docs/BENCHMARKS.md")
    parser.add_argument("--runs", type=int, default=3,
                        help="Number of iterations per test (default: 3)")
    parser.add_argument("--wm-bin", "--mm-bin", dest="wm_bin", type=str, default=None,
                        help="Path to wm binary (default: searches target/release/wm or PATH)")
    parser.add_argument("--markdown", action="store_true",
                        help="Output results as Markdown tables")
    parser.add_argument("--use-hyperfine", action="store_true",
                        help="Use hyperfine for timing if available in PATH")
    args = parser.parse_args()

    # Locate binaries
    mm_bin = args.wm_bin
    if not mm_bin:
        for candidate in ["target/release/wm", "target/release/mm"]:
            local_rel = os.path.abspath(candidate)
            if os.path.exists(local_rel) and os.access(local_rel, os.X_OK):
                mm_bin = local_rel
                break
        if not mm_bin:
            mm_bin = shutil.which("wm") or shutil.which("mm")

    if not mm_bin:
        print("Error: Could not locate 'wm' binary. Run `just install` or `cargo build --release` first.", file=sys.stderr)
        sys.exit(1)

    default_cfg = os.path.abspath("waymaker-cli/assets/config.toml")
    wm_base_cmd = [mm_bin, "--config", default_cfg] if os.path.exists(default_cfg) else [mm_bin]

    fzf_bin = shutil.which("fzf")

    hyperfine_bin = shutil.which("hyperfine") if args.use_hyperfine else None

    if args.audit:
        sizes = [2000000]
        queries = [
            ("narrow", "mod97file12"),
            ("broad", "src/module_5"),
            ("no_match", "xyzzy"),
        ]
    else:
        sizes = args.sizes
        queries = [
            ("exact", "README.md"),
            ("fuzzy_short", "rnst"),
            ("subpath", "render/state"),
            ("broad_prefix", "src"),
            ("no_match", "xyz_nonexistent_token_123"),
        ]

    print(f"=== Waymaker Filter Benchmark Harness ===")
    print(f"Waymaker binary:   {mm_bin}")
    print(f"fzf binary:        {fzf_bin or 'Not found (skipping fzf comparison)'}")
    print(f"Runs per query:    {args.runs}")
    print(f"Corpus sizes:      {sizes}\n")

    temp_dir = tempfile.mkdtemp(prefix="wm_bench_")
    try:
        for size in sizes:
            corpus_file = os.path.join(temp_dir, f"corpus_{size}.txt")
            print(f"Generating synthetic corpus of {size:,} paths...")
            if args.audit:
                generate_audit_corpus(size, corpus_file)
            else:
                generate_corpus(size, corpus_file)

            file_size_mb = os.path.getsize(corpus_file) / (1024 * 1024)
            print(f"Corpus size: {file_size_mb:.2f} MB ({size:,} lines)\n")

            if hyperfine_bin and args.use_hyperfine:
                print(f"--- Running with Hyperfine ({size:,} lines) ---")
                for q_label, query in queries:
                    print(f"\n[Query: {q_label} ('{query}')]")
                    cmd_mm = f"{' '.join(wm_base_cmd)} -f '{query}' < {corpus_file}"
                    hf_cmd = [hyperfine_bin, "--runs", str(args.runs), "--warmup", "1"]
                    if fzf_bin:
                        cmd_fzf = f"{fzf_bin} -f '{query}' < {corpus_file}"
                        hf_cmd.extend(["-n", "wm -f", cmd_mm, "-n", "fzf -f", cmd_fzf])
                    else:
                        hf_cmd.extend(["-n", "wm -f", cmd_mm])
                    subprocess.run(hf_cmd)
                continue

            # Standard timing harness
            results = []

            for q_label, query in queries:
                mm_stats = run_benchmark_command(wm_base_cmd + ["-f", query], corpus_file, args.runs)

                fzf_stats = None
                if fzf_bin:
                    fzf_stats = run_benchmark_command([fzf_bin, "-f", query], corpus_file, args.runs)

                results.append({
                    "label": q_label,
                    "query": query,
                    "mm": mm_stats,
                    "fzf": fzf_stats,
                })

            # Print results
            if args.markdown:
                print(f"### Corpus: {size:,} lines\n")
                if fzf_bin:
                    print("| Query Pattern | Query | wm -f (median) | fzf -f (median) | wm Peak RSS | fzf Peak RSS | Matches |")
                    print("| :--- | :--- | ---: | ---: | ---: | ---: | ---: |")
                    for r in results:
                        mm_t = f"{r['mm']['median_ms']:.1f} ms"
                        fzf_t = f"{r['fzf']['median_ms']:.1f} ms"
                        mm_rss = f"{r['mm']['rss_mib']:.1f} MiB"
                        fzf_rss = f"{r['fzf']['rss_mib']:.1f} MiB"
                        print(f"| `{r['label']}` | `{r['query']}` | {mm_t} | {fzf_t} | {mm_rss} | {fzf_rss} | {r['mm']['lines']:,} |")
                else:
                    print("| Query Pattern | Query | wm -f (median) | Peak RSS | Matches |")
                    print("| :--- | :--- | ---: | ---: | ---: |")
                    for r in results:
                        mm_t = f"{r['mm']['median_ms']:.1f} ms"
                        mm_rss = f"{r['mm']['rss_mib']:.1f} MiB"
                        print(f"| `{r['label']}` | `{r['query']}` | {mm_t} | {mm_rss} | {r['mm']['lines']:,} |")
                print()
            else:
                print(f"{'Query Pattern':<15} {'Query':<16} {'wm -f (ms)':<16} {'fzf -f (ms)':<16} {'wm RSS (MiB)':<14} {'Matches'}")
                print("-" * 90)
                for r in results:
                    mm_str = f"{r['mm']['median_ms']:.1f} ± {r['mm']['stddev_ms']:.1f}"
                    if r['fzf']:
                        fzf_str = f"{r['fzf']['median_ms']:.1f} ± {r['fzf']['stddev_ms']:.1f}"
                    else:
                        fzf_str = "N/A"
                    rss_str = f"{r['mm']['rss_mib']:.1f}"

                    print(f"{r['label']:<15} {r['query']:<16} {mm_str:<16} {fzf_str:<16} {rss_str:<14} {r['mm']['lines']}")
                print()

    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)


if __name__ == "__main__":
    main()
