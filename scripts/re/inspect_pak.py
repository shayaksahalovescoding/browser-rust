"""Read-only Chrome DataPack (.pak v4/v5) inspector."""
import hashlib
import json
import os
import struct
import sys


def inspect_pak(path: str) -> dict:
    size = os.path.getsize(path)
    with open(path, "rb") as f:
        data = f.read()
    sha256 = hashlib.sha256(data).hexdigest()

    version = struct.unpack_from("<I", data, 0)[0]
    if version == 4:
        resource_count = struct.unpack_from("<I", data, 4)[0]
        encoding = data[8]
        alias_count = 0
        table_off = 9
    elif version == 5:
        encoding = data[4]
        resource_count = struct.unpack_from("<H", data, 8)[0]
        alias_count = struct.unpack_from("<H", data, 10)[0]
        table_off = 12
    else:
        raise ValueError(f"Unsupported .pak version {version} in {path}")

    entries = []
    for i in range(resource_count + 1):
        rid, off = struct.unpack_from("<HI", data, table_off + i * 6)
        entries.append((rid, off))

    samples = []
    type_counts = {"gzip": 0, "html": 0, "js_or_json": 0, "css": 0, "png": 0, "svg": 0, "text": 0, "binary": 0}
    for i in range(resource_count):
        rid, start = entries[i]
        end = entries[i + 1][1]
        payload = data[start:end]
        kind = "binary"
        preview = ""
        if payload[:2] == b"\x1f\x8b":
            kind = "gzip"
        elif payload[:8] == b"\x89PNG\r\n\x1a\n":
            kind = "png"
        elif b"<!doctype html" in payload[:128].lower() or b"<html" in payload[:128].lower():
            kind = "html"
            preview = payload[:120].decode("utf-8", errors="replace")
        elif payload.lstrip()[:4] == b"<svg":
            kind = "svg"
            preview = payload[:120].decode("utf-8", errors="replace")
        elif all(32 <= b < 127 or b in (9, 10, 13) for b in payload[:64]) and len(payload) > 0:
            txt = payload[:120].decode("utf-8", errors="replace")
            if "{" in txt and (":" in txt or "function" in txt or "import " in txt):
                kind = "js_or_json"
            elif "color:" in txt or "--" in txt:
                kind = "css"
            else:
                kind = "text"
            preview = txt
        type_counts[kind] += 1
        if len(samples) < 12 and kind in ("html", "js_or_json", "css", "svg"):
            samples.append({"resource_id": rid, "bytes": len(payload), "kind": kind, "preview": preview.replace("\n", " ")[:100]})

    return {
        "path": path,
        "bytes": size,
        "sha256": sha256,
        "pak_version": version,
        "encoding": encoding,
        "resource_count": resource_count,
        "alias_count": alias_count,
        "type_counts": type_counts,
        "samples": samples,
    }


if __name__ == "__main__":
    print(json.dumps([inspect_pak(p) for p in sys.argv[1:]], indent=2))
