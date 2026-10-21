#!/usr/bin/env python3
"""Capture public gameplay on an existing native display (or under xvfb-run)."""
import argparse
from pathlib import Path
import subprocess
import os

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("--out", type=Path, default=ROOT / "evidence" / "campaign-current")
args = parser.parse_args()
out = args.out.resolve()
out.mkdir(parents=True, exist_ok=False)
# Software-review child only. This does not change player or system audio settings.
alsa = out / "alsa-null.conf"
alsa.write_text("pcm.!default { type null }\nctl.!default { type null }\n")
result = subprocess.run(
    [str(args.binary.resolve()), "--size", "1280x800", "--mute", "--capture", str(out / "frames"),
     "--frames", "30,115,250,385,650,745,835,880,1105,1210", "--exit-after", "1220",
     "--script", (ROOT / "assets" / "verification.script").read_text().strip(), "--perf"],
    cwd=ROOT, env={**os.environ, "ALSA_CONFIG_PATH": str(alsa)},
    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
)
(out / "native-campaign.log").write_text(result.stdout)
print(result.stdout)
if result.returncode:
    raise SystemExit(result.returncode)
if "chamber=6 remaining=0 docked=3" not in result.stdout:
    raise SystemExit("Campaign did not finish through public controls; inspect the native captures.")
