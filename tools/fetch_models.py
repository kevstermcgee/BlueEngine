"""Fetch a reviewed CC0 selection into an external directory, with exact checksums."""
import hashlib
import io
import json
from pathlib import Path
import shutil
import tempfile
from urllib.request import urlopen
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def fetch(output, spec=None, opener=urlopen):
    spec = spec or json.loads((ROOT / "assets/model-sources.json").read_text())
    if spec["license"] != "CC0-1.0":
        raise ValueError("fetch accepts reviewed CC0-1.0 sources only")
    output = Path(output).absolute()
    if output.exists():
        raise ValueError("output already exists; fetch preserves earlier packs")
    with opener(spec["archive_url"], timeout=60) as response:
        archive = response.read(spec["download_budget_bytes"] + 1)
    if len(archive) > spec["download_budget_bytes"]:
        raise ValueError("archive exceeds download budget")
    if hashlib.sha256(archive).hexdigest() != spec["archive_sha256"]:
        raise ValueError("archive checksum mismatch; nothing installed, review upstream update")
    selected = {}
    with zipfile.ZipFile(io.BytesIO(archive)) as z:
        for item in spec["files"]:
            name = item["output"]
            if Path(name).name != name or name in selected:
                raise ValueError("invalid/duplicate reviewed output name")
            info = z.getinfo(item["member"])
            if info.file_size > spec["pack_budget_bytes"]:
                raise ValueError("selected member exceeds pack budget")
            data = z.read(info)
            if hashlib.sha256(data).hexdigest() != item["sha256"]:
                raise ValueError("member checksum mismatch: " + name)
            selected[name] = data
    size = sum(map(len, selected.values()))
    if size > spec["pack_budget_bytes"]:
        raise ValueError("selection exceeds pack budget")
    output.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=".cc0-fetch-", dir=output.parent))
    try:
        for name, data in selected.items():
            (stage / name).write_bytes(data)
        (stage / "source-receipt.json").write_text(json.dumps({"version": 1, "source": spec, "bytes": size}, indent=2) + "\n")
        stage.rename(output)
    finally:
        if stage.exists():
            shutil.rmtree(stage)
    return {"directory": str(output), "bytes": size, "files": sorted(selected), "license": spec["license"], "checksum": "verified"}
