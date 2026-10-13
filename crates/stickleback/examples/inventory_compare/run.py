#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Build the actual current example, replay in two processes, save provenance."""

import hashlib
import json
import platform
from pathlib import Path
import statistics
import subprocess
import sys


def invoke(args, root):
    return subprocess.run(args, cwd=root, text=True, capture_output=True, check=True)


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: python3 run.py NEW_OUTPUT_DIRECTORY")
    here = Path(__file__).resolve().parent
    root = here.parents[3]
    output = Path(sys.argv[1]).resolve()
    output.mkdir(parents=True, exist_ok=False)
    build = invoke(["cargo", "build", "--locked", "--offline", "-p", "stickleback",
                    "--example", "inventory_compare", "--message-format=json"], root)
    (output / "build.log").write_text(build.stderr)
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
    executables = [item["executable"] for item in artifacts
                   if item.get("reason") == "compiler-artifact"
                   and item.get("target", {}).get("name") == "inventory_compare"
                   and item.get("executable")]
    if len(executables) != 1:
        raise RuntimeError("Cargo did not report exactly one current example executable")
    executable = Path(executables[0])
    paths = [output / "comparison.json", output / "replay.json"]
    for path in paths:
        result = invoke([str(executable), str(path)], root)
        (output / (path.stem + ".log")).write_text(result.stdout + result.stderr)
    if paths[0].read_bytes() != paths[1].read_bytes():
        raise RuntimeError("two fresh processes did not produce byte-identical reports")
    report = json.loads(paths[0].read_text())
    groups = {}
    for row in report["results"]:
        groups.setdefault((row["scenario"], row["mode"]), []).append(row)
    summary = []
    for (scenario, mode), rows in groups.items():
        def mean(field):
            return statistics.mean(row[field] for row in rows)
        actual_frames = [r["counters"]["control_bytes"]
                         + r["counters"]["attempted_payload_frame_bytes"]
                         + r["counters"]["attempted_result_frame_bytes"] for r in rows]
        summary.append({"scenario": scenario, "mode": mode, "runs": len(rows),
                        "remote_received_percent_mean": mean("remote_received_percent"),
                        "remote_received_percent_min": min(r["remote_received_percent"] for r in rows),
                        "remote_received_percent_max": max(r["remote_received_percent"] for r in rows),
                        "modeled_attempted_frame_bytes_mean": statistics.mean(actual_frames),
                        "control_bytes_mean": statistics.mean(r["counters"]["control_bytes"] for r in rows),
                        "useful_received_payload_bytes_mean": statistics.mean(
                            r["counters"]["useful_received_payload_bytes"] for r in rows),
                        "final_common_objects_mean": mean("final_common_objects"),
                        "unreceived_pairs_mean": mean("unreceived_pairs")})
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    files = [*sorted((root / "crates/stickleback/src/inventory").glob("*.rs")),
             *sorted(here.glob("*.rs")), here / "run.py", root / "Cargo.lock",
             root / "crates/stickleback/Cargo.toml", root / "crates/stickleback/src/lib.rs"]
    metadata = {"schema": "mere.inventory-policy-provenance/v1",
                "base_revision": invoke(["git", "rev-parse", "HEAD"], root).stdout.strip(),
                "source_dirty": bool(invoke(["git", "status", "--porcelain"], root).stdout),
                "toolchain": invoke(["rustc", "-Vv"], root).stdout,
                "platform": platform.platform(),
                "source_sha256": {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
                                  for p in files},
                "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
                "report_sha256": hashlib.sha256(paths[0].read_bytes()).hexdigest(),
                "compiled_source_blake3": report["compiled_source_blake3"],
                "compiled_lock_blake3": report["compiled_lock_blake3"],
                "independent_process_replay_equal": True,
                "runs": len(report["results"])}
    (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    paths[1].unlink()
    print(f"{len(report['results'])} runs; identical fresh-process replay; evidence: {output}")
    for row in summary:
        print(f"{row['scenario']:22} {row['mode']:16} "
              f"{row['remote_received_percent_mean']:6.2f}% "
              f"{row['modeled_attempted_frame_bytes_mean']:10.0f} modeled bytes")


if __name__ == "__main__":
    main()
