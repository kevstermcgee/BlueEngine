"""Validate/render/check Leo's named banks with the supplied engine CLI. No playback or Rust rebuild."""
import argparse
from pathlib import Path
import subprocess

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tools", type=Path)
    parser.add_argument("--output", type=Path, help="New audio directory for revised bundles; existing bundles are never replaced")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1] / "assets"
    output = args.output.resolve() if args.output else root / "audio"
    output.mkdir(parents=True, exist_ok=True)
    for name in ["nature", "music"]:
        project = root / "audio-source" / f"{name}.json"
        bundle = output / name
        base = [str(args.tools.resolve()), "audio"]
        subprocess.run(base + ["validate", str(project)], check=True)
        subprocess.run(base + ["render", str(project), str(bundle)], check=True)
        subprocess.run(base + ["check", str(bundle)], check=True)

if __name__ == "__main__":
    main()
