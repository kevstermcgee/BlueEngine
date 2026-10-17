#!/usr/bin/env python3
"""Compatibility entry point; shared implementation is pinned in tooling.json."""
from pathlib import Path
import importlib.util
_spec = importlib.util.spec_from_file_location("be2_bootstrap", Path(__file__).with_name("tooling.py"))
_bootstrap = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_bootstrap)
_impl = _bootstrap.load('check', Path(__file__).resolve().parents[1])
globals().update({k: v for k, v in vars(_impl).items() if not k.startswith("__")})
if __name__ == "__main__":
    import sys
    sys.exit(_impl.main(root=Path(__file__).resolve().parents[1]))
