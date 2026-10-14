#!/usr/bin/env python3
"""
BlueEngine MCP Server launcher.
Delegates stdio to the high-performance native Rust MCP server (`be2-tools mcp`).
Ensures seamless compatibility with Claude Desktop, Cursor, Cline, and any MCP stdio client.
"""

import os
import sys
import subprocess
from pathlib import Path
try:
    from tools.game_tooling import fresh_tools
except ModuleNotFoundError:
    from game_tooling import fresh_tools

REPO_ROOT = Path(__file__).resolve().parent.parent

def find_or_build_binary() -> Path:
    # Share Cargo invalidation, explicit-override validation and target discovery with game tooling.
    # A stale release executable must not hide newly indexed public contracts from MCP clients.
    return Path(fresh_tools(REPO_ROOT, os.environ.get('BE2_TOOLS')))

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
