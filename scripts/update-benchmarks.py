#!/usr/bin/env python3
"""Run benchmarks one by one and update html/index.html with fresh results."""

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HTML_PATH = os.path.join(ROOT, "html", "index.html")

# ─── Benchmark execution ───────────────────────────────────────────────────────


def run_bench(test_name):
    """Run a single benchmark and return its stderr output."""
    print(f"  Running {test_name}...", end="", flush=True)
    r = subprocess.run(
        ["cargo", "test", "--release", "--test", "copy_bench", test_name, "--", "--nocapture"],
        capture_output=True, text=True, timeout=600, cwd=ROOT,
    )
    if r.returncode != 0:
        print(f" FAILED (exit {r.returncode})")
        return None
    print(" ok")
    return r.stderr


def parse_times(output):
    """Extract label -> time_ms from 'LABEL: X.XXms avg (N runs)' lines."""
    times = {}
    for m in re.finditer(r"  (.+?): (\d+(?:\.\d+)?)ms avg", output):
        times[m.group(1).strip()] = float(m.group(2))
    return times


def parse_startup(output):
    m = re.search(r"startup overhead: ~(\d+(?:\.\d+)?)ms", output)
    return float(m.group(1)) if m else None


# ─── Collect all results ───────────────────────────────────────────────────────

BENCH_DEFS = [
    # (test_name, result_key, gnu_label, our_label)
    ("bench_many_small_files",  "many_small",  "GNU cp -R", "our cp -R"),
    ("bench_preserve_metadata", "recursive",   "GNU cp -a", "our cp -a"),
    ("bench_deep_tree",         "deep_tree",   "GNU cp -R", "our cp -R"),
    ("bench_mixed_sizes",       "mixed",       "GNU cp -R", "our cp -R"),
    ("bench_large_file_100mb",  "large_file",  "GNU cp",    "our cp (copy_file_range)"),
    ("bench_hardlink_heavy",    "hardlink",    "GNU cp -a", "our cp -a"),
    ("bench_symlink_heavy",     "symlink",     "GNU cp -R", "our cp -R"),
    ("bench_sparse_file",       "sparse",      "GNU cp",    "our cp (sparse=auto)"),
]


def collect_results():
    results = {}

    for test_name, key, gnu_label, our_label in BENCH_DEFS:
        out = run_bench(test_name)
        if out:
            t = parse_times(out)
            gnu = t.get(gnu_label)
            ours = t.get(our_label)
            if gnu and ours:
                results[key] = {"gnu": round(gnu, 1), "ours": round(ours, 1)}

    # Parallel threshold
    out = run_bench("bench_parallel_threshold")
    if out:
        t = parse_times(out)
        threshold = {}
        for count in [32, 64, 128, 256]:
            key = f"our cp -R ({count} files)"
            if key in t:
                threshold[str(count)] = round(t[key], 1)
        if threshold:
            results["threshold"] = threshold

    # Startup
    out = run_bench("bench_single_file_startup")
    if out:
        s = parse_startup(out)
        if s:
            results["startup_ms"] = round(s, 1)

    return results


# ─── HTML update ───────────────────────────────────────────────────────────────

def fmt(ms):
    """Format time for display in the HTML."""
    if ms >= 1000:
        return f"{ms / 1000:.1f} s"
    if ms >= 100:
        return f"{ms:.0f} ms"
    return f"{ms:.1f} ms"


# Comment markers for each bench card in order
CARD_MARKERS = [
    ("<!-- 1. Many small files -->", "many_small"),
    ("<!-- 2. Recursive -a -->",     "recursive"),
    ("<!-- 3. Deep tree -->",        "deep_tree"),
    ("<!-- 4. Mixed sizes -->",      "mixed"),
    ("<!-- 5. Large file -->",       "large_file"),
    ("<!-- 6. Hardlink heavy -->",   "hardlink"),
    ("<!-- 7. Symlink heavy -->",    "symlink"),
    ("<!-- 8. Sparse -->",           "sparse"),
]


def update_bench_card(lines, start, gnu_ms, our_ms):
    """Update a single bench card starting at line index `start`."""
    speedup = gnu_ms / our_ms

    # Bar widths and classes
    if speedup >= 0.95:
        gnu_width = 100
        our_width = min(100, max(2, round(our_ms / gnu_ms * 100)))
        our_extra_class = ""
    else:
        our_width = 100
        gnu_width = min(100, max(2, round(gnu_ms / our_ms * 100)))
        our_extra_class = " slower"

    if speedup >= 1.05:
        spd_class, spd_label = "fast", "faster"
    elif speedup >= 0.95:
        spd_class, spd_label = "par", "on par"
    else:
        spd_class, spd_label = "slow", "scan overhead"

    # Scan forward from `start` to update the 6 target lines
    gnu_bar_done = False
    our_bar_done = False
    gnu_time_done = False
    our_time_done = False
    spd_done = False
    lbl_done = False

    for i in range(start, min(start + 40, len(lines))):
        line = lines[i]

        # GNU bar width
        if not gnu_bar_done and "bench-bar-fill gnu" in line and "style=" in line:
            lines[i] = re.sub(r'width: \d+%', f'width: {gnu_width}%', line)
            gnu_bar_done = True
            continue

        # GNU time
        if not gnu_time_done and gnu_bar_done and "bench-bar-time" in line:
            lines[i] = re.sub(r'>[\d.]+ ms<', f'>{fmt(gnu_ms)}<', line)
            gnu_time_done = True
            continue

        # Our bar width + class
        if not our_bar_done and "bench-bar-fill ours" in line and "style=" in line:
            lines[i] = re.sub(
                r'bench-bar-fill ours\s*(?:slower\s*)?"',
                f'bench-bar-fill ours{our_extra_class}"',
                line,
            )
            lines[i] = re.sub(r'width: \d+%', f'width: {our_width}%', lines[i])
            our_bar_done = True
            continue

        # Our time
        if not our_time_done and our_bar_done and "bench-bar-time" in line:
            lines[i] = re.sub(r'>[\d.]+ ms<', f'>{fmt(our_ms)}<', line)
            our_time_done = True
            continue

        # Speedup value
        if not spd_done and "speedup-value" in line:
            lines[i] = re.sub(
                r'speedup-value \w+">[\d.]+x',
                f'speedup-value {spd_class}">{speedup:.1f}x',
                line,
            )
            spd_done = True
            continue

        # Speedup label
        if not lbl_done and "speedup-label" in line:
            lines[i] = re.sub(r'>[\w ]+<', f'>{spd_label}<', line)
            lbl_done = True
            break

    return lines


def update_threshold(lines, threshold):
    """Update the parallel threshold bar heights and times."""
    max_ms = max(threshold.values())

    for count_str, ms in threshold.items():
        pct = max(10, round(ms / max_ms * 90))
        # Find the threshold-label with this count
        for i, line in enumerate(lines):
            if f'threshold-label">{count_str}<' in line:
                # Walk backwards to find the fill height and time
                for j in range(i - 1, max(i - 8, 0), -1):
                    if "threshold-fill" in lines[j] and "height:" in lines[j]:
                        lines[j] = re.sub(r'height: \d+%', f'height: {pct}%', lines[j])
                    if "t-time" in lines[j]:
                        lines[j] = re.sub(r'>[\d.]+ ms<', f'>{fmt(ms)}<', lines[j])
                break


def update_startup(lines, startup_ms):
    """Update startup overhead value."""
    for i, line in enumerate(lines):
        if 'startup-val">' in line:
            lines[i] = re.sub(r'~[\d.]+ ms', f'~{fmt(startup_ms)}', line)
            break


def update_html(results):
    with open(HTML_PATH) as f:
        lines = f.read().split("\n")

    # Update each bench card
    for marker, key in CARD_MARKERS:
        if key not in results:
            continue
        for i, line in enumerate(lines):
            if marker in line:
                update_bench_card(lines, i, results[key]["gnu"], results[key]["ours"])
                break

    # Threshold
    if "threshold" in results:
        update_threshold(lines, results["threshold"])

    # Startup
    if "startup_ms" in results:
        update_startup(lines, results["startup_ms"])

    with open(HTML_PATH, "w") as f:
        f.write("\n".join(lines))

    print("  html/index.html updated.")


# ─── Main ──────────────────────────────────────────────────────────────────────

def main():
    dry_run = "--dry-run" in sys.argv

    print("Running benchmarks (one by one)...\n")
    results = collect_results()

    # Save JSON for reference
    json_path = os.path.join(ROOT, "bench-results.json")
    with open(json_path, "w") as f:
        json.dump(results, f, indent=2)
    print(f"\nResults → {json_path}")

    # Summary
    print("\n┌─────────────────────────────────────────────────────────────┐")
    print("│  Benchmark              GNU          Ours       Speedup    │")
    print("├─────────────────────────────────────────────────────────────┤")
    for key, data in results.items():
        if isinstance(data, dict) and "gnu" in data:
            g, o = data["gnu"], data["ours"]
            s = g / o
            marker = "▲" if s >= 1.05 else ("▼" if s < 0.95 else "─")
            print(f"│  {key:20s}  {fmt(g):>8s}    {fmt(o):>8s}    {marker} {s:.1f}x     │")
    if "startup_ms" in results:
        print(f"│  {'startup':20s}     —        ~{fmt(results['startup_ms']):>7s}       —        │")
    print("└─────────────────────────────────────────────────────────────┘")

    if not dry_run:
        print("\nUpdating HTML...")
        update_html(results)
    else:
        print("\n(dry-run mode — HTML not modified)")


if __name__ == "__main__":
    main()
