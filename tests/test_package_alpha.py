import hashlib
import io
import tarfile
import tempfile
import unittest
import re
import subprocess
from pathlib import Path

import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import package_alpha
import package_windows_alpha as shared_notices


class PackageAlphaTests(unittest.TestCase):
    def test_empty_inherited_encoded_rustflags_are_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "externally supplied Rust build flags"):
            package_alpha.compiler_environment({"CARGO_ENCODED_RUSTFLAGS": "", "KEEP_ME": "yes"})
        env = package_alpha.compiler_environment({"KEEP_ME": "yes"})
        self.assertEqual(env["KEEP_ME"], "yes")
        self.assertTrue(env["RUSTFLAGS"].startswith("--remap-path-prefix="))

    def test_allowlist_rejects_extra_missing_and_traversal_members(self):
        valid = package_alpha.MEMBERS + ("BUILD-INFO.json", "SHA256SUMS")
        self.assertEqual(set(package_alpha.validate_members(valid)), set(valid))
        for names in (valid[:-1], valid + ("diagnostic",), valid[:-1] + ("../escape",)):
            with self.subTest(names=names):
                with self.assertRaises(ValueError):
                    package_alpha.validate_members(names)

    def test_input_symlink_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target"
            target.write_text("synthetic", encoding="utf-8")
            link = root / "link"
            link.symlink_to(target)
            with self.assertRaises(ValueError):
                package_alpha.require_regular_inputs({"fixture": link})

    def test_nested_extraction_and_readme_link_rewrite(self):
        contents = {name: ("fixture " + name).encode() for name in package_alpha.MEMBERS}
        contents["BUILD-INFO.json"] = b"{}\n"
        contents["SHA256SUMS"] = b""
        guide = b"See [agent skill](../skills/binus-cli/SKILL.md)."
        self.assertEqual(package_alpha.archive_readme(guide), b"See [agent skill](skills/binus-cli/SKILL.md).")
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "fixture.tar.gz"
            with tarfile.open(archive, "w:gz") as tar:
                for name, content in contents.items():
                    item = tarfile.TarInfo(name)
                    item.size = len(content)
                    item.mode = 0o555 if name == "binus" else 0o444
                    tar.addfile(item, io.BytesIO(content))
            root = Path(directory) / "extract"
            package_alpha.extract_archive(archive, root)
            self.assertEqual((root / "skills/binus-cli/SKILL.md").read_bytes(), contents["skills/binus-cli/SKILL.md"])
            symlink_archive = Path(directory) / "unsafe.tar.gz"
            with tarfile.open(symlink_archive, "w:gz") as tar:
                for name, content in contents.items():
                    item = tarfile.TarInfo(name)
                    item.mode = 0o555 if name == "binus" else 0o444
                    if name == "skills/binus-cli/SKILL.md":
                        item.type = tarfile.SYMTYPE
                        item.linkname = "../../outside"
                        tar.addfile(item)
                    else:
                        item.size = len(content)
                        tar.addfile(item, io.BytesIO(content))
            with self.assertRaisesRegex(ValueError, "non-regular"):
                package_alpha.extract_archive(symlink_archive, Path(directory) / "rejected")

    def test_exclusive_output_preserves_existing_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "archive"
            path.write_bytes(b"pre-existing")
            with self.assertRaises(FileExistsError):
                package_alpha.create_exclusive(path, b"replacement")
            self.assertEqual(path.read_bytes(), b"pre-existing")

    def test_linux_elf_identity_and_truncation(self):
        valid = bytearray(64)
        valid[:6] = b"\x7fELF\x02\x01"
        valid[16:18] = (3).to_bytes(2, "little")
        valid[18:20] = (62).to_bytes(2, "little")
        package_alpha.validate_elf_x86_64(bytes(valid))
        for invalid in (bytes(valid[:19]), bytes(valid[:4]) + b"\x01\x01" + bytes(valid[6:]), bytes(valid[:18]) + b"\x03\x00"):
            with self.subTest(invalid=invalid[:20]), self.assertRaisesRegex(ValueError, "ELF64 x86_64"):
                package_alpha.validate_elf_x86_64(invalid)

    def test_project_license_is_a_committed_packager_input(self):
        license_bytes = package_alpha.require_regular_inputs({"LICENSE": package_alpha.ROOT / "LICENSE"})["LICENSE"]
        blob = package_alpha.run(["git", "show", f"HEAD:{(package_alpha.ROOT / 'LICENSE').relative_to(package_alpha.ROOT)}"], cwd=package_alpha.ROOT).stdout.encode()
        self.assertEqual(license_bytes, blob)
        self.assertEqual(package_alpha.SOURCE_FILES["LICENSE"], package_alpha.ROOT / "LICENSE")

    def test_bounded_notice_reader_rejects_missing_and_oversized_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, "missing"):
                shared_notices.read_bounded_regular(root / "missing", 4)
            too_large = root / "large"
            too_large.write_bytes(b"12345")
            with self.assertRaisesRegex(ValueError, "exceeds bounded size"):
                shared_notices.read_bounded_regular(too_large, 4)

    def test_production_writer_verify_extract_round_trip(self):
        contents = {name: ("synthetic fixture " + name).encode() for name in package_alpha.MEMBERS}
        contents["README.md"] = package_alpha.archive_readme(
            b"Read [agent skill](../skills/binus-cli/SKILL.md)."
        )
        contents["BUILD-INFO.json"] = b'{"fixture":"synthetic"}\n'
        contents["SHA256SUMS"] = "".join(
            f"{hashlib.sha256(contents[name]).hexdigest()}  {name}\n"
            for name in package_alpha.CHECKSUM_MEMBERS
        ).encode("ascii")
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "production-writer.tar.gz"
            package_alpha.create_archive(archive, contents, "2026-10-01T12:00:00+00:00")
            verified = package_alpha.verify_archive(archive, contents)
            extracted_path = Path(directory) / "extracted"
            package_alpha.extract_archive(archive, extracted_path)
            self.assertEqual(verified, contents)
            for name in package_alpha.MEMBERS + ("BUILD-INFO.json", "SHA256SUMS"):
                self.assertEqual((extracted_path / name).read_bytes(), contents[name])
            with tarfile.open(archive, "r:gz") as tar:
                self.assertEqual({item.name for item in tar.getmembers()}, set(contents))
                self.assertTrue(all(item.uid == item.gid == 0 and not item.uname and not item.gname for item in tar.getmembers()))

    def test_documented_extraction_layout_with_production_archive(self):
        contents = {name: ("synthetic fixture " + name).encode() for name in package_alpha.MEMBERS}
        contents["README.md"] = package_alpha.archive_readme(
            b"Read [agent skill](../skills/binus-cli/SKILL.md)."
        )
        contents["BUILD-INFO.json"] = b'{"fixture":"synthetic"}\n'
        contents["SHA256SUMS"] = "".join(
            f"{hashlib.sha256(contents[name]).hexdigest()}  {name}\n"
            for name in package_alpha.CHECKSUM_MEMBERS
        ).encode("ascii")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "synthetic.tar.gz"
            package_alpha.create_archive(archive, contents, "2026-10-01T12:00:00+00:00")
            guide = (package_alpha.ROOT / "docs/LINUX.md").read_text(encoding="utf-8")
            commands = re.search(r"```sh\n(.*?)\n```", guide, re.S).group(1)
            self.assertIn("mkdir binus-alpha", commands)
            self.assertIn("tar -xzf binus-v0.1.0-alpha.1-linux-x86_64.tar.gz -C binus-alpha", commands)
            self.assertIn("cd binus-alpha", commands)
            destination = root / "binus-alpha"
            destination.mkdir()
            subprocess.run(["tar", "-xzf", str(archive), "-C", str(destination)], check=True)
            self.assertEqual((destination / "binus").read_bytes(), contents["binus"])
            self.assertFalse((destination / "binus-alpha").exists())

    def test_archive_integrity_and_nonregular_member_rejection(self):
        contents = {name: ("synthetic " + name).encode() for name in package_alpha.MEMBERS}
        info = b'{"fixture":"synthetic"}\n'
        contents["BUILD-INFO.json"] = info
        checksummed = package_alpha.MEMBERS + ("BUILD-INFO.json",)
        contents["SHA256SUMS"] = "".join(
            f"{hashlib.sha256(contents[name]).hexdigest()}  {name}\n" for name in checksummed
        ).encode("ascii")
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "synthetic.tar.gz"
            def write_archive(target, replacements=None, symlink_name=None):
                replacements = replacements or {}
                with tarfile.open(target, "w:gz") as tar:
                    for name, content in contents.items():
                        item = tarfile.TarInfo(name)
                        item.mode = 0o555 if name == "binus" else 0o444
                        if name == symlink_name:
                            item.type = tarfile.SYMTYPE
                            item.linkname = "outside"
                            tar.addfile(item)
                            continue
                        body = replacements.get(name, content)
                        item.size = len(body)
                        tar.addfile(item, io.BytesIO(body))
            with tarfile.open(archive, "w:gz") as tar:
                for name, content in contents.items():
                    item = tarfile.TarInfo(name)
                    item.size = len(content)
                    item.mode = 0o555 if name == "binus" else 0o444
                    tar.addfile(item, io.BytesIO(content))
            result = package_alpha.verify_archive(archive, contents)
            self.assertEqual(result, contents)
            tampered = dict(contents)
            tampered["binus"] = b"synthetic modified binary"
            with self.assertRaisesRegex(ValueError, "content mismatch"):
                package_alpha.verify_archive(archive, tampered)
            symlink_archive = Path(directory) / "symlink.tar.gz"
            write_archive(symlink_archive, symlink_name="skills/binus-cli/SKILL.md")
            with self.assertRaisesRegex(ValueError, "non-regular"):
                package_alpha.verify_archive(symlink_archive, contents)
            corrupt_archive = Path(directory) / "corrupt-checksum.tar.gz"
            bad_sums = contents["SHA256SUMS"].replace(b"binus", b"xxxxx", 1)
            write_archive(corrupt_archive, {"SHA256SUMS": bad_sums})
            with self.assertRaises((ValueError, KeyError)):
                package_alpha.verify_archive(corrupt_archive, {**contents, "SHA256SUMS": bad_sums})
            wrong_digest_archive = Path(directory) / "wrong-digest.tar.gz"
            wrong_digest_sums = contents["SHA256SUMS"].replace(
                hashlib.sha256(contents["binus"]).hexdigest().encode("ascii"), b"0" * 64, 1
            )
            write_archive(wrong_digest_archive, {"SHA256SUMS": wrong_digest_sums})
            with self.assertRaisesRegex(ValueError, "^archive checksum mismatch: binus$"):
                package_alpha.verify_archive(
                    wrong_digest_archive, {**contents, "SHA256SUMS": wrong_digest_sums}
                )


if __name__ == "__main__":
    unittest.main()
