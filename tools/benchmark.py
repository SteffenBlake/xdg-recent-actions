#!/usr/bin/env python3
"""Compare release binaries using isolated data/state directories and GNU time."""
import argparse
import datetime
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import tempfile
import time
import xml.etree.ElementTree as ET


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--variant", action="append", required=True, metavar="NAME=BINARY")
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--time", default="/usr/bin/time", help="GNU time executable")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.runs < 2:
        parser.error("--runs must be at least 2")
    variants = {}
    for value in args.variant:
        name, binary = value.split("=", 1)
        if not re.fullmatch(r"[a-zA-Z0-9_-]+", name) or name in variants:
            parser.error("variant names must be unique simple directory names")
        variants[name] = str(Path(binary).resolve(strict=True))
    if not Path(args.time).is_file():
        parser.error("GNU time is required; supply --time /path/to/time")

    with tempfile.TemporaryDirectory(prefix="xdg-recent-bench-") as temporary:
        root = Path(temporary)
        data = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
        state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
        originals = root / "originals"
        originals.mkdir()
        manifest_path = state / "xdg-recent-actions/state.json"
        manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {"entries": {}}
        for file in (data / "applications").glob("*.desktop"):
            if file.name in manifest["entries"]:
                record = manifest["entries"][file.name]
                if manifest["version"] == 2:
                    record = json.loads((manifest_path.parent / "entries" / (file.name + ".json")).read_text())
                if record["original"] is not None:
                    (originals / file.name).write_text(record["original"])
            else:
                shutil.copyfile(file, originals / file.name)

        files = root / "files"
        files.mkdir()
        small, large = ["<xbel>"], ["<xbel>"]
        base = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
        for item in range(10_000):
            path = files / str(item)
            path.touch()
            # Deliberately unsorted timestamps, unique within each application's history.
            stamp = (base + datetime.timedelta(seconds=(item * 7919) % 10_000)).isoformat()
            small.append(f'<bookmark href="{path.as_uri()}"><application name="bench{item % 100}" exec="bench{item % 100} %u" modified="{stamp}"/></bookmark>')
            large.append(f'<bookmark href="{path.as_uri()}">')
            for app in range((item % 10) * 10, (item % 10) * 10 + 10):
                large.append(f'<application name="bench{app}" exec="bench{app} %u" modified="{stamp}"/>')
            large.append("</bookmark>")
        small.append("</xbel>")
        large.append("</xbel>")
        cases = []
        history = data / "recently-used.xbel"
        if history.exists():
            snapshot = root / "actual.xbel"
            shutil.copyfile(history, snapshot)
            cases.append(("actual history", snapshot, None))
        for label, content, payload in [
            ("10k records / 100 apps", small, 0),
            ("100k records / 100 apps", large, 0),
            ("10k records / 100 apps / 64KiB desktop entries", small, 65_536),
        ]:
            case = root / f"case{len(cases)}"
            (case / "system/applications").mkdir(parents=True)
            for app in range(100):
                (case / f"system/applications/bench{app}.desktop").write_text(
                    f"[Desktop Entry]\nName=bench{app}\nType=Application\nExec=bench{app} %U\n" + ("#" + "x" * payload + "\n" if payload else ""))
            (case / "history.xbel").write_text("".join(content))
            cases.append((label, case / "history.xbel", case / "system"))
        del small, large

        results = []
        for case_number, (label, history, system) in enumerate(cases):
            environments, samples, first = {}, {}, {}
            for name, binary in variants.items():
                case = root / f"run{case_number}" / name
                (case / "data/applications").mkdir(parents=True)
                shutil.copyfile(history, case / "data/recently-used.xbel")
                if system is None:
                    shutil.copytree(originals, case / "data/applications", dirs_exist_ok=True)
                environments[name] = os.environ | {"XDG_DATA_HOME": str(case / "data"), "XDG_STATE_HOME": str(case / "state")}
                if system is not None:
                    environments[name]["XDG_DATA_DIRS"] = str(system)
                samples[name] = []
                first[name] = measure(args.time, binary, environments[name], root / "metrics")
            # Alternate order between rounds to reduce order and scheduling bias.
            for run in range(args.runs):
                for name in (list(variants) if run % 2 == 0 else list(reversed(variants))):
                    samples[name].append(measure(args.time, variants[name], environments[name], root / "metrics"))
            for name, rows in samples.items():
                result = {"case": label, "variant": name, "runs": args.runs,
                          "median_wall_ms": statistics.median(r["wall_ms"] for r in rows),
                          "median_processing_ms": statistics.median(r["processing_ms"] for r in rows),
                          "peak_rss_kib": max(r["rss_kib"] for r in rows), "first_sync": first[name]}
                results.append(result)
                print(json.dumps(result), flush=True)
        report = {"method": "GNU time peak child RSS; wall time includes process and time launcher startup; warm filesystem cache; no systemd activation overhead", "results": results}
        if args.output:
            args.output.write_text(json.dumps(report, indent=2) + "\n")


def measure(timer, binary, environment, metrics):
    start = time.perf_counter_ns()
    run = subprocess.run([timer, "-o", str(metrics), "-f", "%M", binary, "sync"], env=environment, text=True, capture_output=True)
    elapsed = (time.perf_counter_ns() - start) / 1e6
    if run.returncode:
        raise RuntimeError(run.stdout + run.stderr)
    processing = re.search(r"Finished in ([0-9.]+) ms", run.stdout)
    return {"wall_ms": elapsed, "processing_ms": float(processing[1]), "rss_kib": int(metrics.read_text())}


if __name__ == "__main__":
    main()
