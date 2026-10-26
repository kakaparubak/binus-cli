import hashlib
import io
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import package_windows_alpha as windows


def fixture_pe(machine=0x8664):
    data = bytearray(0x200)
    data[:2] = b"MZ"
    data[0x3c:0x40] = (0x80).to_bytes(4, "little")
    data[0x80:0x84] = b"PE\0\0"
    data[0x84:0x86] = machine.to_bytes(2, "little")
    data[0x86:0x88] = (0).to_bytes(2, "little")
    data[0x94:0x96] = (0xF0).to_bytes(2, "little")
    data[0x98:0x9a] = (0x20B).to_bytes(2, "little")
    data[0x98 + 108:0x98 + 112] = (16).to_bytes(4, "little")
    return bytes(data)


def fixture_pe_import(dll=b"kernel32.dll"):
    data = bytearray(fixture_pe())
    optional = 0x98
    data[optional + 112:optional + 120] = bytes(8)  # export directory remains empty
    data[optional + 120:optional + 124] = (0x1000).to_bytes(4, "little")
    data[optional + 124:optional + 128] = (40).to_bytes(4, "little")
    # One import descriptor followed by its null terminator; DLL name at RVA 0x1030.
    data.extend(bytes(0x200))
    data[0x200:0x20c] = bytes(12)
    data[0x20c:0x210] = (0x1038).to_bytes(4, "little")
    data[0x210:0x220] = bytes(16)
    data[0x238:0x238 + len(dll) + 1] = dll + b"\0"
    # PE header declares one section; its raw data begins at file offset 0x200.
    data[0x86:0x88] = (1).to_bytes(2, "little")
    data[0x188 + 8:0x188 + 12] = (0x2000).to_bytes(4, "little")
    data[0x188 + 12:0x188 + 16] = (0x1000).to_bytes(4, "little")
    data[0x188 + 16:0x188 + 20] = (0x200).to_bytes(4, "little")
    data[0x188 + 20:0x188 + 24] = (0x200).to_bytes(4, "little")
    return bytes(data)


def fixture_pe_import_section(dll_rva, raw_size=0x400, virtual_size=0x400, file_size=None):
    data = bytearray(fixture_pe_import())
    data.extend(bytes(max(0, 0x200 + raw_size - len(data))))
    data[0x188 + 8:0x188 + 12] = virtual_size.to_bytes(4, "little")
    data[0x188 + 16:0x188 + 20] = raw_size.to_bytes(4, "little")
    data[0x20c:0x210] = dll_rva.to_bytes(4, "little")
    name_pos = 0x200 + dll_rva - 0x1000
    data[name_pos:name_pos + 13] = b"kernel32.dll\0"
    if file_size is not None:
        del data[file_size:]
    return bytes(data)


class WindowsAlphaTests(unittest.TestCase):
    def contents(self):
        data = {name: ("synthetic " + name).encode() for name in windows.CHECKSUM_MEMBERS}
        data["BUILD-INFO.json"] = b'{"fixture":"synthetic"}\n'
        data["SHA256SUMS"] = "".join(f"{hashlib.sha256(data[n]).hexdigest()}  {n}\n" for n in windows.CHECKSUM_MEMBERS).encode()
        return data

    def write(self, path, contents, names=None, symlink=None):
        with zipfile.ZipFile(path, "w") as zf:
            for name in (names or windows.MEMBERS):
                info = zipfile.ZipInfo(name)
                info.external_attr = (0o120777 if name == symlink else 0o100644) << 16
                zf.writestr(info, contents.get(name, b"x"))

    def test_archive_gate_red_then_green(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            archive = root / "synthetic.zip"
            contents = self.contents()
            self.write(archive, contents)
            self.assertEqual(windows.verify_zip(archive), contents)
            # Red: change a validly named member while leaving its committed digest stale.
            broken = dict(contents)
            broken["binus.exe"] += b"corruption"
            self.write(archive, broken)
            with self.assertRaisesRegex(ValueError, "archive checksum mismatch: binus.exe"):
                windows.verify_zip(archive)
            # Green: recompute the member hashes and validate the completed archive.
            fixed = dict(contents)
            fixed["binus.exe"] = broken["binus.exe"]
            fixed["SHA256SUMS"] = "".join(f"{hashlib.sha256(fixed[n]).hexdigest()}  {n}\n" for n in windows.CHECKSUM_MEMBERS).encode()
            self.write(archive, fixed)
            self.assertEqual(windows.verify_zip(archive), fixed)

    def test_production_writer_output_passes_verification_and_extraction(self):
        with tempfile.TemporaryDirectory() as td:
            archive, dest = Path(td) / "production.zip", Path(td) / "extract"
            contents = self.contents()
            with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zf:
                windows.write_zip_contents(zf, contents)
            self.assertEqual(windows.verify_zip(archive), contents)
            self.assertEqual(windows.extract_zip(archive, dest), contents)
            self.assertEqual((dest / "LICENSE").read_bytes(), contents["LICENSE"])
            self.assertEqual((dest / "RUST-STDLIB-LICENSES.html").read_bytes(), contents["RUST-STDLIB-LICENSES.html"])

    def test_license_is_a_committed_packager_input(self):
        self.assertEqual(windows.DOCS["LICENSE"], "LICENSE")
        license_bytes = windows.require_regular(windows.ROOT / "LICENSE")
        with patch.object(windows, "git_blob", return_value=license_bytes) as committed_blob:
            windows.verify_worktree_doc(windows.require_regular(windows.ROOT / windows.DOCS["LICENSE"]), committed_blob("synthetic-commit", windows.DOCS["LICENSE"]), "LICENSE")
        committed_blob.assert_called_once_with("synthetic-commit", "LICENSE")

    def test_missing_license_and_checksum_entry_are_rejected(self):
        with tempfile.TemporaryDirectory() as td:
            archive = Path(td) / "missing-license.zip"
            contents = self.contents()
            self.write(archive, contents, names=tuple(n for n in windows.MEMBERS if n != "LICENSE"))
            with self.assertRaisesRegex(ValueError, "allowlist"):
                windows.verify_zip(archive)
            contents["SHA256SUMS"] = "".join(f"{hashlib.sha256(contents[n]).hexdigest()}  {n}\n" for n in windows.CHECKSUM_MEMBERS if n != "LICENSE").encode()
            self.write(archive, contents)
            with self.assertRaisesRegex(ValueError, "incomplete"):
                windows.verify_zip(archive)

    def test_directory_mode_on_allowed_filename_is_rejected(self):
        with tempfile.TemporaryDirectory() as td:
            archive = Path(td) / "directory-mode.zip"
            contents = self.contents()
            with zipfile.ZipFile(archive, "w") as zf:
                for name in windows.MEMBERS:
                    info = zipfile.ZipInfo(name)
                    info.external_attr = (0o040755 << 16) | 0x10 if name == "binus.exe" else 0o100644 << 16
                    zf.writestr(info, contents[name])
            with self.assertRaisesRegex(ValueError, "non-regular"):
                windows.verify_zip(archive)

    def test_allowlist_duplicate_traversal_and_symlink_rejected(self):
        with self.assertRaises(ValueError): windows.validate_members(windows.MEMBERS + ("binus.exe",))
        with self.assertRaises(ValueError): windows.validate_members(windows.MEMBERS[:-1] + ("../escape",))
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "bad.zip"
            self.write(path, self.contents(), symlink="binus.exe")
            with self.assertRaisesRegex(ValueError, "non-regular"):
                windows.verify_zip(path)

    def test_nested_extraction_and_readme_link_rewrite(self):
        self.assertEqual(windows.archive_readme(b"[agent skill](../skills/binus-cli/SKILL.md)"), b"[agent skill](skills/binus-cli/SKILL.md)")
        with tempfile.TemporaryDirectory() as td:
            archive, dest = Path(td) / "x.zip", Path(td) / "extract"
            contents = self.contents()
            self.write(archive, contents)
            windows.extract_zip(archive, dest)
            self.assertEqual((dest / "skills/binus-cli/SKILL.md").read_bytes(), contents["skills/binus-cli/SKILL.md"])

    def test_pe_architecture_and_truncation(self):
        self.assertEqual(windows.parse_pe_x64(fixture_pe()), [])
        with self.assertRaisesRegex(ValueError, "not x86_64"):
            windows.parse_pe_x64(fixture_pe(0x14c))
        with self.assertRaisesRegex(ValueError, "truncated"):
            windows.parse_pe_x64(fixture_pe()[:70])

    def test_pe_import_directory_uses_import_entry_and_validates_name(self):
        self.assertEqual(windows.parse_pe_x64(fixture_pe_import()), ["kernel32.dll"])
        self.assertEqual(windows.parse_pe_x64(fixture_pe_import(b"api-ms-win-core-file-l1-1-0.dll")), ["api-ms-win-core-file-l1-1-0.dll"])
        self.assertEqual(windows.non_inbox_imports(["api-ms-win-core-file-l1-1-0.dll", "ext-ms-win-ntuser-window-l1-1-0.dll"]), [])
        self.assertEqual(windows.non_inbox_imports(["vcruntime140.dll"]), ["vcruntime140.dll"])
        with self.assertRaisesRegex(ValueError, "invalid PE import DLL name"):
            windows.parse_pe_x64(fixture_pe_import(b"bad name.dll"))

    def test_pe_import_names_are_bounded_by_backed_raw_section(self):
        self.assertEqual(windows.parse_pe_x64(fixture_pe_import_section(0x1380)), ["kernel32.dll"])
        with self.assertRaisesRegex(ValueError, "outside sections"):
            windows.parse_pe_x64(fixture_pe_import_section(0x1380, raw_size=0x200, virtual_size=0x400))
        with self.assertRaisesRegex(ValueError, "truncated"):
            windows.parse_pe_x64(fixture_pe_import_section(0x1380, file_size=0x500))

    def test_doc_normalization_only_allows_crlf_difference(self):
        committed = b"alpha\nbeta\n"
        windows.verify_worktree_doc(b"alpha\r\nbeta\r\n", committed, "README.md")
        with self.assertRaisesRegex(SystemExit, "README.md"):
            windows.verify_worktree_doc(b"changed\r\nbeta\r\n", committed, "README.md")
        with self.assertRaisesRegex(SystemExit, "README.md"):
            windows.verify_worktree_doc(b"alpha\rbeta\r", committed, "README.md")

    def test_audit_checks_real_marker_and_path_encodings(self):
        values = windows.audit_values()
        self.assertIn(b"auth_browser", values)
        self.assertIn(b"binus-auth-debug", values)
        for path in (str(Path.home()), str(windows.ROOT)):
            for form in {path, path.replace("/", "\\"), path.replace("\\", "/")}:
                encoded = form.encode("utf-8")
                self.assertIn(encoded, values)
                self.assertIn(encoded.decode("utf-8").encode("utf-16le"), values)

    def test_exclusive_output_preserves_existing_file(self):
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "artifact.zip"
            path.write_bytes(b"pre-existing")
            with self.assertRaises(FileExistsError):
                windows.create_exclusive(path, b"replacement")
            self.assertEqual(path.read_bytes(), b"pre-existing")

    def test_notice_file_is_bounded_and_confined_to_package_root(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "crate"
            root.mkdir()
            (root / "LICENSE.txt").write_text("synthetic license", encoding="utf-8")
            self.assertEqual(windows.bounded_notice_files(root)["LICENSE.txt"], "synthetic license")
            with self.assertRaisesRegex(ValueError, "escapes"):
                windows.bounded_notice_files(root, "../outside-license.txt")

    def test_rust_stdlib_notice_is_required_regular_and_bounded(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            notice = root / "share/doc/rust/COPYRIGHT-library.html"
            notice.parent.mkdir(parents=True)
            notice.write_bytes(b"synthetic public notice")
            self.assertEqual(windows.rust_stdlib_notice(root), b"synthetic public notice")
            notice.unlink()
            with self.assertRaisesRegex(ValueError, "missing"):
                windows.rust_stdlib_notice(root)
            notice.mkdir()
            with self.assertRaisesRegex(ValueError, "regular"):
                windows.rust_stdlib_notice(root)
            notice.rmdir()
            notice.write_bytes(b"x" * (2 * 1024 * 1024 + 1))
            with self.assertRaisesRegex(ValueError, "bounded size"):
                windows.rust_stdlib_notice(root)


if __name__ == "__main__": unittest.main()
