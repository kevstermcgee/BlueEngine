#!/usr/bin/env python3
"""
BlueEngine MCP Server launcher.
Delegates stdio to the high-performance native Rust MCP server (`be2-tools mcp`).
Ensures seamless compatibility with Claude Desktop, Cursor, Cline, and any MCP stdio client.
"""

import os
import sys
import shutil
import subprocess
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

def find_or_build_binary() -> Path:
    ext = ".exe" if sys.platform == "win32" else ""
    release_bin = REPO_ROOT / "target" / "release" / f"be2-tools{ext}"
    debug_bin = REPO_ROOT / "target" / "debug" / f"be2-tools{ext}"

    if release_bin.exists():
        return release_bin
    if debug_bin.exists():
        return debug_bin

    which = shutil.which(f"be2-tools{ext}")
    if which:
        return Path(which)

    # Build debug binary on first run if cargo is present
    cargo = shutil.which("cargo")
    if cargo:
        sys.stderr.write("[be2-mcp] be2-tools binary not found, compiling with cargo...\n")
        sys.stderr.flush()
        res = subprocess.run([cargo, "build", "--bin", "be2-tools"], cwd=REPO_ROOT)
        if res.returncode == 0 and debug_bin.exists():
            return debug_bin

    raise RuntimeError("Could not find or build `be2-tools` binary. Run `cargo build --bin be2-tools` first.")

def main():
    try:
        binary = find_or_build_binary()
    except Exception as e:
        sys.stderr.write(f"[be2-mcp] Error: {e}\n")
        sys.stderr.flush()
        sys.exit(1)

    # Pass stdin/stdout directly to native Rust MCP server
    proc = subprocess.Popen(
        [str(binary), "mcp"],
        stdin=sys.stdin,
        stdout=sys.stdout,
        stderr=sys.stderr,
        cwd=REPO_ROOT
    )
    sys.exit(proc.wait())

if __name__ == "__main__":
    main()
