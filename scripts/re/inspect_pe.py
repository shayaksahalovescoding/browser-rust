"""Read-only PE header, section, import/export, and Rust-marker inspector for Chrome binaries."""
import hashlib
import json
import os
import re
import struct
import sys


def read_u16(buf: bytes, off: int) -> int:
    return struct.unpack_from("<H", buf, off)[0]


def read_u32(buf: bytes, off: int) -> int:
    return struct.unpack_from("<I", buf, off)[0]


def read_u64(buf: bytes, off: int) -> int:
    return struct.unpack_from("<Q", buf, off)[0]


def rva_to_offset(sections: list[dict], rva: int) -> int | None:
    for s in sections:
        va = s["virtual_address"]
        size = max(s["virtual_size"], s["raw_size"])
        if va <= rva < va + size:
            raw_off = rva - va + s["raw_ptr"]
            if raw_off < s["raw_ptr"] + s["raw_size"]:
                return raw_off
            return None
    return None


def read_cstr(f, offset: int, limit: int = 256) -> str:
    f.seek(offset)
    data = f.read(limit)
    nul = data.find(b"\x00")
    if nul != -1:
        data = data[:nul]
    return data.decode("ascii", errors="replace")


def inspect_pe(path: str) -> dict:
    size = os.path.getsize(path)
    sha256 = hashlib.sha256()
    rustc_commits = set()
    has_dep_v0 = False
    rustc_re = re.compile(rb"/rustc/([0-9a-f]{40})/")

    with open(path, "rb") as f:
        header = f.read(4096)
        f.seek(0)
        tail = b""
        while True:
            chunk = f.read(4 * 1024 * 1024)
            if not chunk:
                break
            sha256.update(chunk)
            window = tail + chunk
            for m in rustc_re.finditer(window):
                rustc_commits.add(m.group(1).decode("ascii"))
            tail = chunk[-64:]

        if header[:2] != b"MZ":
            raise ValueError(f"Not MZ PE: {path}")
        pe_off = read_u32(header, 0x3C)
        if pe_off + 264 > len(header):
            f.seek(pe_off)
            pe_hdr = f.read(512)
        else:
            pe_hdr = header[pe_off:]
        if pe_hdr[:4] != b"PE\x00\x00":
            raise ValueError(f"Missing PE signature: {path}")

        machine = read_u16(pe_hdr, 4)
        num_sections = read_u16(pe_hdr, 6)
        opt_hdr_size = read_u16(pe_hdr, 20)
        magic = read_u16(pe_hdr, 24)
        is_pe32_plus = magic == 0x20B
        entry_rva = read_u32(pe_hdr, 24 + 16)
        dd_off = 24 + (112 if is_pe32_plus else 96)
        export_rva = read_u32(pe_hdr, dd_off)
        export_size = read_u32(pe_hdr, dd_off + 4)
        import_rva = read_u32(pe_hdr, dd_off + 8)
        import_size = read_u32(pe_hdr, dd_off + 12)
        delay_import_rva = read_u32(pe_hdr, dd_off + 13 * 8)

        sec_table_off = pe_off + 24 + opt_hdr_size
        f.seek(sec_table_off)
        sec_bytes = f.read(num_sections * 40)
        sections = []
        for i in range(num_sections):
            sb = sec_bytes[i * 40 : (i + 1) * 40]
            name = sb[:8].split(b"\x00", 1)[0].decode("ascii", errors="replace")
            if name == ".dep-v0":
                has_dep_v0 = True
            sections.append(
                {
                    "name": name,
                    "virtual_size": read_u32(sb, 8),
                    "virtual_address": read_u32(sb, 12),
                    "raw_size": read_u32(sb, 16),
                    "raw_ptr": read_u32(sb, 20),
                }
            )

        imports = []
        if import_rva:
            imp_off = rva_to_offset(sections, import_rva)
            if imp_off is not None:
                idx = 0
                while idx < 256:
                    f.seek(imp_off + idx * 20)
                    desc = f.read(20)
                    if len(desc) < 20 or desc == b"\x00" * 20:
                        break
                    name_rva = read_u32(desc, 12)
                    name_off = rva_to_offset(sections, name_rva)
                    if name_off is not None:
                        imports.append(read_cstr(f, name_off))
                    idx += 1

        delay_imports = []
        if delay_import_rva:
            dimp_off = rva_to_offset(sections, delay_import_rva)
            if dimp_off is not None:
                idx = 0
                while idx < 256:
                    f.seek(dimp_off + idx * 32)
                    desc = f.read(32)
                    if len(desc) < 32 or desc == b"\x00" * 32:
                        break
                    name_rva = read_u32(desc, 4)
                    name_off = rva_to_offset(sections, name_rva)
                    if name_off is not None:
                        delay_imports.append(read_cstr(f, name_off))
                    idx += 1

        exports = []
        export_count = 0
        if export_rva:
            exp_off = rva_to_offset(sections, export_rva)
            if exp_off is not None:
                f.seek(exp_off)
                exp_dir = f.read(40)
                num_names = read_u32(exp_dir, 24)
                names_rva = read_u32(exp_dir, 32)
                export_count = num_names
                names_off = rva_to_offset(sections, names_rva)
                if names_off is not None:
                    limit = min(num_names, 30)
                    f.seek(names_off)
                    name_rvas = f.read(limit * 4)
                    for i in range(limit):
                        nrva = read_u32(name_rvas, i * 4)
                        noff = rva_to_offset(sections, nrva)
                        if noff is not None:
                            exports.append(read_cstr(f, noff))

    return {
        "path": path,
        "bytes": size,
        "sha256": sha256.hexdigest(),
        "machine": hex(machine),
        "pe32_plus": is_pe32_plus,
        "entry_rva": hex(entry_rva),
        "sections": sections,
        "imports": imports,
        "delay_imports": delay_imports,
        "export_name_count": export_count,
        "export_sample": exports,
        "has_dep_v0_section": has_dep_v0,
        "rustc_commits_observed": sorted(rustc_commits),
    }


if __name__ == "__main__":
    results = [inspect_pe(p) for p in sys.argv[1:]]
    print(json.dumps(results, indent=2))
