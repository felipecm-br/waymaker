#!/usr/bin/env python3
"""Compare two `wm` binaries on the same input: output, memory and speed.

    scripts/regress.py OLD_WM NEW_WM [--lines N] [--runs R] [--markdown]

- Output: `wm -f QUERY` must print byte-identical results for every query (fails otherwise).
- Memory: peak RSS of `wm -f` may grow at most 10% (fails otherwise).
- Speed: median `wm -f` time and median `--benchmark-input` p99 frame time. Shared CI runners
  are noisy, so only a large slowdown fails: more than 50% slower, or a p99 frame over the
  16.67 ms (60 fps) budget. A warning needs both a ratio and an absolute difference (time:
  over 1.25x and 10 ms; p99: over 1.5x and 4 ms), since identical builds differ by more than
  15% at tens of milliseconds and by 3x at sub-millisecond p99s.

Both binaries run with the shipped config (`waymaker-cli/assets/config.toml`; OLD_WM can get its
own with --old-config) on a generated corpus, so results do not depend on the machine's own
config. Exit status 1 on any failure.
"""
import argparse, os, re, resource, statistics, subprocess, sys, tempfile, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CONFIG = os.path.join(ROOT, "waymaker-cli", "assets", "config.toml")
QUERIES = ["mod97file12", "rs", "src", "zzzqqq", "file_1 mod", "^src", "7$", "'module_9", "src !file_1"]
TIMED = ["zzzqqq", "mod97file12", "rs"]

def corpus(path, n):
    with open(path, "w") as f:
        for i in range(1, n + 1):
            f.write(f"src/module_{i % 977}/file_{i}.rs path/to/something/{i * 7}\n")

CONFIGS = {}  # binary -> the config it runs with (see --old-config)

def run(binary, args, stdin_path):
    """(stdout bytes, seconds) of one run."""
    with open(stdin_path, "rb") as stdin:
        t = time.perf_counter()
        p = subprocess.run([binary, "--config", CONFIGS.get(binary, CONFIG), *args], stdin=stdin,
                           stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        return p.stdout, time.perf_counter() - t

def peak_rss_mib(binary, args, stdin_path):
    """Peak RSS of one run, measured in a fresh Python process: RUSAGE_CHILDREN is a running
    maximum over all children, so it cannot be read per run in this process."""
    code = ("import resource,subprocess,sys;"
            "subprocess.run(sys.argv[1:-1],stdin=open(sys.argv[-1],'rb'),"
            "stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);"
            "print(resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)")
    out = subprocess.run([sys.executable, "-c", code, binary, "--config", CONFIGS.get(binary, CONFIG), *args, stdin_path],
                         capture_output=True, text=True, check=True).stdout
    kib = int(out.strip())
    return kib / 1024 if sys.platform != "darwin" else kib / (1024 * 1024)

def p99_frame_ms(binary, stdin_path):
    with open(stdin_path, "rb") as stdin:
        out = subprocess.run([binary, "--config", CONFIGS.get(binary, CONFIG), "--benchmark-input", "mod97"],
                             stdin=stdin, capture_output=True, text=True).stdout
    m = re.search(r"p99 Frametime:\s+([\d.]+)\s*ms", out)
    return float(m.group(1)) if m else float("nan")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("old"); ap.add_argument("new")
    ap.add_argument("--lines", type=int, default=500_000)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--markdown", action="store_true")
    ap.add_argument("--old-config", help="config for OLD_WM (default: this checkout's); an older "
                    "binary may not parse a newer config, e.g. one using an action added since")
    a = ap.parse_args()
    if a.old_config:
        CONFIGS[a.old] = a.old_config

    failures, warnings, rows = [], [], []
    with tempfile.TemporaryDirectory() as tmp:
        big = os.path.join(tmp, "corpus.txt")
        small = os.path.join(tmp, "frames.txt")
        corpus(big, a.lines)
        corpus(small, min(a.lines, 100_000))

        for q in QUERIES:
            old_out, _ = run(a.old, ["-f", q], big)
            new_out, _ = run(a.new, ["-f", q], big)
            same = old_out == new_out
            rows.append(("output", f"`-f {q}`", f"{old_out.count(b'\n')} lines",
                         f"{new_out.count(b'\n')} lines", "same" if same else "**DIFFERENT**"))
            if not same:
                failures.append(f"`wm -f {q}` output differs")
            if q != "zzzqqq" and not new_out:
                failures.append(f"`wm -f {q}` printed nothing; the comparison would be vacuous")

        for q in TIMED:
            ot = statistics.median(run(a.old, ["-f", q], big)[1] for _ in range(a.runs))
            nt = statistics.median(run(a.new, ["-f", q], big)[1] for _ in range(a.runs))
            ratio = nt / ot if ot else float("inf")
            verdict = "ok"
            if ratio > 1.5:
                verdict = "**FAIL**"; failures.append(f"`wm -f {q}` {ratio:.2f}x slower")
            elif ratio > 1.25 and nt - ot > 0.010:
                verdict = "warn"; warnings.append(f"`wm -f {q}` {ratio:.2f}x slower")
            rows.append(("time", f"`-f {q}` median", f"{ot * 1000:.0f} ms", f"{nt * 1000:.0f} ms",
                         f"{ratio:.2f}x {verdict}"))

            om, nm = peak_rss_mib(a.old, ["-f", q], big), peak_rss_mib(a.new, ["-f", q], big)
            mratio = nm / om if om else float("inf")
            verdict = "ok"
            if mratio > 1.10:
                verdict = "**FAIL**"; failures.append(f"`wm -f {q}` peak RSS {mratio:.2f}x")
            rows.append(("memory", f"`-f {q}` peak RSS", f"{om:.0f} MiB", f"{nm:.0f} MiB",
                         f"{mratio:.2f}x {verdict}"))

        op = statistics.median(p99_frame_ms(a.old, small) for _ in range(3))
        np_ = statistics.median(p99_frame_ms(a.new, small) for _ in range(3))
        verdict = "ok"
        if not np_ < 16.67:
            verdict = "**FAIL**"; failures.append(f"p99 frame {np_:.1f} ms is over the 16.67 ms budget")
        elif op and np_ / op > 1.5 and np_ > 4.0:  # sub-millisecond p99s are jitter
            verdict = "warn"; warnings.append(f"p99 frame {np_ / op:.2f}x slower")
        rows.append(("frames", "p99 frame, streaming 100k", f"{op:.2f} ms", f"{np_:.2f} ms", verdict))

    if a.markdown:
        print(f"## Old vs new `wm` ({a.lines:,} lines)\n")
        print("| Check | What | Old | New | Result |\n| :--- | :--- | ---: | ---: | :--- |")
        for r in rows:
            print("| " + " | ".join(r) + " |")
        print()
        for w in warnings:
            print(f"- warning: {w}")
        for f in failures:
            print(f"- **failure: {f}**")
    else:
        for r in rows:
            print(f"{r[0]:7} {r[1]:28} {r[2]:>12} {r[3]:>12}  {r[4]}")
        for w in warnings:
            print("warning:", w)
        for f in failures:
            print("FAILURE:", f)
    sys.exit(1 if failures else 0)

if __name__ == "__main__":
    main()
