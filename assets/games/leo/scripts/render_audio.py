"""Render Leo's reproducible banks before packaging; never opens an audio device."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from uuid import uuid4
from prepare_ambience import prepare_leaves


def audio_command(root, tools=None):
    if tools:
        return [str(Path(tools).resolve()), "audio"]
    if os.environ.get("BE2_TOOLS"):
        return [os.environ["BE2_TOOLS"], "audio"]
    from check import engine_path, find_tools
    engine = engine_path(root)
    if engine and (engine / "tools/be2.py").is_file():
        # Canonical source authoring builds fresh tooling with Cargo's invalidation.
        return [sys.executable, str(engine / "tools/be2.py"), "map", "audio"]
    found = find_tools(root)
    if found:
        return [found, "audio"]
    raise RuntimeError("Leo audio rendering needs the engine checkout or BE2_TOOLS")


def render(root, output, tools=None):
    source = root / "assets/audio-source"
    prepare_leaves(source / "leaves.wav")
    base = audio_command(root, tools)
    output.mkdir(parents=True, exist_ok=True)
    for name in ("nature", "music"):
        project, bundle = source / f"{name}.json", output / name
        for arguments in (["validate", str(project)], ["render", str(project), str(bundle)],
                          ["check", str(bundle)]):
            subprocess.run(base + arguments, check=True)
        # Preview mixes are authoring outputs; runtime uses the bank's individual layers.
        (bundle / "preview-mix.wav").unlink(missing_ok=True)


def ensure_audio(root, tools=None):
    """Render/check everything before replacing an earlier completed generated bundle."""
    assets = root / "assets"
    destination = assets / "audio"
    backup = assets / (".audio-old-" + uuid4().hex)
    with tempfile.TemporaryDirectory(prefix=".audio-render-", dir=assets) as temporary:
        staged = Path(temporary) / "audio"
        render(root, staged, tools)
        if destination.exists():
            destination.rename(backup)
        try:
            staged.rename(destination)
        except OSError:
            if backup.exists():
                backup.rename(destination)
            raise
        if backup.exists():
            shutil.rmtree(backup)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tools", type=Path, nargs="?")
    parser.add_argument("--output", type=Path, help="New directory; existing outputs are never overwritten")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if args.output:
        render(root, args.output.resolve(), args.tools)
    else:
        ensure_audio(root, args.tools)


if __name__ == "__main__":
    main()
