#!/usr/bin/env python3
"""Forward to the engine's supported web workflow; no copied deployment logic."""
import subprocess,sys,tomllib
from pathlib import Path
game=Path(__file__).resolve().parent.parent
manifest=tomllib.loads((game/'Cargo.toml').read_text())
engine=(game/manifest['dependencies']['vesper3d']['path']).resolve()
sys.exit(subprocess.call([sys.executable,str(engine/'tools/web_games.py'),*sys.argv[1:2],str(game),*sys.argv[2:]]))
