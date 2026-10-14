"""Small standalone bootstrap for the game's pinned BlueEngine project tooling."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys

FILES = ('templates/game_check.py', 'templates/game_dev.py', 'templates/game_project.py',
         'templates/game_ship.py', 'templates/game_installer.py', 'templates/game_review.py',
         'templates/game_entry.py', 'tools/game_tooling.py')


def tooling_digest(engine):
    digest = hashlib.sha256()
    for name in FILES:
        digest.update(name.encode('utf-8') + b'\0')
        digest.update((Path(engine) / name).read_bytes() + b'\0')
    return digest.hexdigest()


def load(command, root):
    root = Path(root).resolve()
    try:
        pin = json.loads((root / 'scripts/tooling.json').read_text(encoding='utf-8'))
        if pin.get('schema_version') != 1:
            raise ValueError('unsupported tooling pin schema (expected 1)')
        if not isinstance(pin.get('sha256'), str):
            raise ValueError('tooling pin is unverified; the engine dependency was unavailable during scaffolding')
        engine = (root / os.environ.get('BE2_TOOLING_ROOT', pin['engine'])).resolve()
        if tooling_digest(engine) != pin['sha256']:
            raise ValueError(f'tooling content differs from the pin for {pin.get("revision", "unknown revision")}')
        spec = importlib.util.spec_from_file_location('be2_game_tooling', engine / 'tools/game_tooling.py')
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        return module.load(command, root, engine)
    except (OSError, ValueError, KeyError) as error:
        raise SystemExit(f'BlueEngine tooling: {error}. Restore the engine checkout named by '
                         'scripts/tooling.json, or review the tooling update and run '
                         'python ENGINE/tools/game_tooling.py migrate GAME. Completed outputs were preserved.') from error
