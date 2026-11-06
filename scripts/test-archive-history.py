"""Synthetic tests for the historical PE archive transformation."""
import importlib.util
from pathlib import Path
import struct
import unittest

spec = importlib.util.spec_from_file_location(
    "archive_history", Path(__file__).with_name("archive-historical-preview.py")
)
archive = importlib.util.module_from_spec(spec)
spec.loader.exec_module(archive)


def fixture():
    home = Path("C:/Users/tester")
    data = bytearray(4096)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 128)
    data[128:132] = b"PE\0\0"
    struct.pack_into("<H", data, 134, 3)
    struct.pack_into("<II", data, 140, 3072, 0)
    struct.pack_into("<H", data, 148, 240)
    struct.pack_into("<H", data, 152, 0x20B)
    for i, (name, start, flags) in enumerate([
        (b".text", 1024, 0x60000020),
        (b".rdata", 1536, 0x40000040),
        (b".rsrc", 2560, 0x40000040),
    ]):
        pos = 392 + 40 * i
        data[pos:pos + len(name)] = name
        struct.pack_into("<II", data, pos + 16, 512, start)
        struct.pack_into("<I", data, pos + 36, flags)
    data[1024:1536] = b"A" * 512
    text = f"{home}\\source\\app\\main.rs\0local.tester.switcher-pet\0".replace("/", "\\").encode()
    data[1536:1536 + len(text)] = text
    company = "CompanyName\0\0tester\0".encode("utf-16le")
    data[2560:2560 + len(company)] = company
    symbol = b"__C__Users_tester_source_app_main\0"
    struct.pack_into("<I", data, 3072, 4 + len(symbol))
    data[3076:3076 + len(symbol)] = symbol
    return bytes(data), home


class ArchiveTests(unittest.TestCase):
    def test_equal_length_and_code_preserved(self):
        original, home = fixture()
        result, paths, symbols, resources = archive.redact_pe(original, home)
        self.assertEqual(len(result), len(original))
        self.assertEqual(result[1024:1536], original[1024:1536])
        self.assertEqual((paths, symbols, resources), (1, 1, 1))
        self.assertNotIn(b"tester", result)
        self.assertNotIn("tester".encode("utf-16le"), result)
        self.assertIn(b"local.xxxxxx.switcher-pet", result)

    def test_reject_signed_binary(self):
        original, home = fixture()
        data = bytearray(original)
        struct.pack_into("<II", data, 152 + 112 + 32, 4000, 32)
        with self.assertRaisesRegex(AssertionError, "Signed"):
            archive.redact_pe(bytes(data), home)

    def test_reject_writable_or_executable_path_section(self):
        original, home = fixture()
        for flags in (0xC0000040, 0x60000040):
            data = bytearray(original)
            struct.pack_into("<I", data, 432 + 36, flags)
            with self.assertRaisesRegex(AssertionError, "Executable/writable"):
                archive.redact_pe(bytes(data), home)

    def test_reject_runtime_home_path(self):
        original, home = fixture()
        with self.assertRaisesRegex(AssertionError, "compiler source"):
            archive.redact_pe(original.replace(b"\\source\\", b"\\config\\"), home)

    def test_reject_unclassified_identity(self):
        original, home = fixture()
        data = bytearray(original)
        data[1900:1906] = b"tester"
        with self.assertRaisesRegex(AssertionError, "Unclassified"):
            archive.redact_pe(bytes(data), home)


if __name__ == "__main__":
    unittest.main()
