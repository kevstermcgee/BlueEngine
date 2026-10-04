"""Render Leo and submit its two audio banks only on opted-in remote CI's null sink."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if sys.platform != "linux" or os.environ.get("CI") != "true" or os.environ.get("BE2_AUDIO_OFFSCREEN") != "1" or not os.environ.get("DISPLAY"):
        parser.error("requires remote Linux CI, BE2_AUDIO_OFFSCREEN=1 and a virtual DISPLAY")
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    alsa = output / "null-alsa.conf"
    alsa.write_text("pcm.!default { type null }\n")
    env = {**os.environ, "ALSA_CONFIG_PATH": str(alsa), "LIBGL_ALWAYS_SOFTWARE": "1"}
    binary = args.binary.resolve()
    assets = root / "assets/games/leo/assets"
    def run(label, script, frames, end, *, source=assets, extra=()):
        capture = output / label
        command = [str(binary), "--assets", str(source), "--capture", str(capture), "--frames", frames,
                   "--exit-after", str(end), "--day-seconds", "4", "--seed", "7", "--size", "960x600",
                   "--settings", str(output / "settings.json"), "--save-dir", str(output / "saves"),
                   "--script", script, "--perf", *extra]
        result = subprocess.run(command, env=env, cwd=root, text=True, capture_output=True, timeout=600)
        (output / f"{label}.log").write_text(result.stdout + result.stderr)
        return result, capture
    positive, capture = run("cycle", "look:1.4/-0.12@0,fwd:1-320,sprint:1-320,right:90-140,left:220-260,jump@90,music@60,music@80,save@340,menu@500",
                            "0,55,65,85,125,175,240,340,498,501", 503, extra=["--verify-audio"])
    if positive.returncode:
        raise RuntimeError(f"Leo capture failed ({positive.returncode}): {positive.stderr[-2000:]}")
    report = json.loads((capture / "run.json").read_text())
    rows = report["state"]
    assert report["audio_submissions"] == 503 and not report["muted"]
    assert all(row["audio_ready"] and row["chunks"] == 49 for row in rows)
    assert rows[-1]["days"] >= 2 and rows[-1]["paused"]
    assert not next(r for r in rows if r["frame"] == 65)["music_on"]
    assert next(r for r in rows if r["frame"] == 85)["music_on"]
    assert any(row["origin"] != {"x": 0, "z": 0} for row in rows)
    assert any(.65 < row["phase"] < .85 for row in rows), "sunset evidence"
    assert any(row["phase"] > .9 or row["phase"] < .1 for row in rows), "star evidence"
    for row in rows:
        assert (capture / f"shot_{row['frame']:05d}.png").stat().st_size > 1000
    # Resume the saved day/position through the same load operation as F9, then show the menu.
    saved = next(r for r in rows if r["frame"] == 340)
    resumed, resumed_dir = run("resume", "menu@0", "0", 1, extra=["--load", "quick", "--portrait"])
    assert resumed.returncode == 0, resumed.stderr
    restored = json.loads((resumed_dir / "run.json").read_text())["state"][0]
    # Save occurs before the frame's executed movement step, so its exact clock precedes this frame's capture.
    assert restored["tick"] == saved["tick"] - 1 and restored["days"] == saved["days"]
    assert restored["origin"] == saved["origin"] and restored["paused"]
    assert json.loads((output / "settings.json").read_text())["music_on"] is True
    # A corrupt bank fails explicitly; a normal silent capture proves only rendering and never submits audio.
    broken = output / "broken-assets"
    shutil.copytree(assets / "audio", broken / "audio")
    (broken / "audio/nature/music-birds.wav").unlink()
    negative, _ = run("negative", "menu@0", "0", 1, source=broken, extra=["--verify-audio"])
    assert negative.returncode != 0 and "music-birds.wav" in negative.stderr, negative.stderr
    muted, muted_dir = run("muted", "", "0", 1, source=broken, extra=["--mute", "--portrait"])
    assert muted.returncode == 0, muted.stderr
    assert json.loads((muted_dir / "run.json").read_text())["audio_submissions"] == 0
    evidence = {"ok": True, "platform": "Linux CI / virtual display / software GL / null ALSA", "frames": 503,
                "days": rows[-1]["days"], "audio_submissions": report["audio_submissions"],
                "save_resume": True, "music_toggle_persisted": True, "negative_exit": negative.returncode,
                "audibility_verified": False}
    (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps(evidence))

if __name__ == "__main__":
    main()
