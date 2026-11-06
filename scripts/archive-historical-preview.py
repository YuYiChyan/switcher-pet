"""Create a privacy-redacted archival copy of an early Windows preview.

Accepts equal-length compiler-path replacements in read-only PE .rdata and
COFF strings, plus anonymization of the old personal application identifier
and CompanyName resource. Executable sections and layout stay byte-identical. This is an
archival transformation, not a source rebuild or current production build.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zipfile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def redact_pe(original, home):
    data = bytearray(original)
    assert data[:2] == b"MZ", "Expected PE executable"
    pe = struct.unpack_from("<I", data, 60)[0]
    assert data[pe:pe + 4] == b"PE\0\0"
    opt = pe + 24
    assert struct.unpack_from("<H", data, opt)[0] == 0x20B, "Expected PE32+"
    assert struct.unpack_from("<II", data, opt + 112 + 32) == (0, 0), "Signed input requires rebuilding"
    n = struct.unpack_from("<H", data, pe + 6)[0]
    size = struct.unpack_from("<H", data, pe + 20)[0]
    sections = []
    for i in range(n):
        p = opt + size + i * 40
        name = bytes(data[p:p + 8]).rstrip(b"\0")
        length, start = struct.unpack_from("<II", data, p + 16)
        flags = struct.unpack_from("<I", data, p + 36)[0]
        sections.append((name, start, start + length, flags))
    changes = 0
    for sep in ("\\", "/"):
        prefix = str(home).replace("\\", sep)
        replacement = prefix.rsplit(sep, 1)[0] + sep + "x" * len(home.name)
        for encoding in ("utf-8", "utf-16le"):
            needle, new = prefix.encode(encoding), replacement.encode(encoding)
            assert len(needle) == len(new)
            pos = 0
            while (pos := data.find(needle, pos)) >= 0:
                end = pos + len(needle)
                section = next((s for s in sections if s[1] <= pos and end <= s[2]), None)
                assert section and section[0] == b".rdata"
                assert not section[3] & (0x20000000 | 0x80000000), "Executable/writable data rejected"
                suffix = (sep + "source" + sep).encode(encoding)
                assert data[end:end + len(suffix)] == suffix, "Only compiler source paths accepted"
                data[pos:end] = new
                changes += 1
                pos = end
    assert changes, "No known compiler paths found"
    symbols, symbol_count = struct.unpack_from("<II", data, pe + 12)
    strings = symbols + symbol_count * 18
    string_size = struct.unpack_from("<I", data, strings)[0]
    assert strings + string_size <= len(data)
    symbol_prefix = str(home).replace(":", "_").replace("\\", "_") + "_source_"
    symbol_new = symbol_prefix.replace(home.name, "x" * len(home.name))
    symbol_changes = 0
    pos = strings
    while (pos := data.find(symbol_prefix.encode(), pos, strings + string_size)) >= 0:
        data[pos:pos + len(symbol_prefix)] = symbol_new.encode()
        symbol_changes += 1
        pos += len(symbol_prefix)
    app_id = f"local.{home.name}.switcher-pet".encode()
    app_new = f"local.{'x' * len(home.name)}.switcher-pet".encode()
    assert data.count(app_id) == 1, "Unexpected application identifier"
    pos = data.index(app_id)
    section = next((s for s in sections if s[1] <= pos and pos + len(app_id) <= s[2]), None)
    assert section and section[0] == b".rdata" and not section[3] & (0x20000000 | 0x80000000)
    data[pos:pos + len(app_id)] = app_new
    resource_changes = 0
    company = home.name.encode("utf-16le")
    for name, start, end, flags in sections:
        if name != b".rsrc":
            continue
        pos = start
        while (pos := data.find(company, pos, end)) >= 0:
            assert b"C\x00o\x00m\x00p\x00a\x00n\x00y\x00N\x00a\x00m\x00e\x00" in data[max(start, pos - 32):pos]
            assert not flags & (0x20000000 | 0x80000000)
            data[pos:pos + len(company)] = ("x" * len(home.name)).encode("utf-16le")
            resource_changes += 1
            pos += len(company)
    for name, start, end, flags in sections:
        if name not in (b".rdata", b".rsrc"):
            assert data[start:end] == original[start:end], "Unexpected section mutation"
    assert len(data) == len(original)
    for enc in ("utf-8", "utf-16le"):
        assert home.name.lower().encode(enc) not in bytes(data).lower(), "Unclassified personal identifier"
    # Recalculate the PE checksum after the same-length string substitutions.
    checksum_at = opt + 64
    struct.pack_into("<I", data, checksum_at, 0)
    total = 0
    for pos in range(0, len(data), 2):
        total += int.from_bytes(data[pos:pos + 2], "little")
        total = (total & 0xFFFF) + (total >> 16)
    total = (total & 0xFFFF) + (total >> 16)
    struct.pack_into("<I", data, checksum_at, total + len(data))
    return bytes(data), changes, symbol_changes, resource_changes


def archive(source, destination, home):
    assert source.resolve() != destination.resolve()
    assert not destination.exists(), "Archive exists; retain it rather than overwrite"
    allowed = {
        "使用说明.txt", "Launch.ps1", "LICENSE-pet-assets.txt", "LICENSE-upstream.txt",
        "NOTICE-pet-assets.md", "Open-preview.cmd", "Open-settings.cmd",
        "pet-assets-manifest.json", "SHA256.json", "switcher-pet.exe", "WebView2Loader.dll",
    }
    with zipfile.ZipFile(source) as z:
        assert len(z.namelist()) == len(set(z.namelist()))
        assert set(z.namelist()) <= allowed, "Unexpected package payload"
        files = {name: z.read(name) for name in z.namelist()}
    old = files["switcher-pet.exe"]
    files["switcher-pet.exe"], count, symbol_count, resource_count = redact_pe(old, home)
    provenance = {
        "kind": "historical-build-path-redacted",
        "original_package_sha256": sha(source.read_bytes()),
        "original_executable_sha256": sha(old),
        "archived_executable_sha256": sha(files["switcher-pet.exe"]),
        "compiler_path_replacements": count,
        "compiler_symbol_replacements": symbol_count,
        "personal_application_identifier_anonymized": True,
        "company_name_resource_replacements": resource_count,
        "executable_sections_preserved": True,
        "source_provenance": "Early previews were consolidated in preview 14; exact per-preview source snapshots were not retained.",
        "validation": "Static PE integrity and privacy checks. Anonymized application identity can select a different WebView data directory; use the current release for daily use.",
    }
    files["ARCHIVE-PROVENANCE.json"] = (json.dumps(provenance, indent=2) + "\n").encode()
    files["SHA256.json"] = (json.dumps([
        {"File": name, "SHA256": sha(data).upper()}
        for name, data in sorted(files.items()) if name != "SHA256.json"
    ], ensure_ascii=False, indent=2) + "\n").encode()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination, "x", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, data in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 11, 6, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, data)
    digest = sha(destination.read_bytes())
    destination.with_suffix(".zip.sha256").write_text(f"{digest}  {destination.name}\n", encoding="utf-8")
    return {"file": destination.name, "sha256": digest, "bytes": destination.stat().st_size, **provenance}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--build-home", type=Path, default=Path.home())
    args = parser.parse_args()
    print(json.dumps(archive(args.source, args.destination, args.build_home), indent=2))
