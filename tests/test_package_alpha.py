import hashlib
import io
import tarfile
import tempfile
import unittest
from pathlib import Path

import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import package_alpha


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
