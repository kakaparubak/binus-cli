#!/usr/bin/env python3
"""Build and verify a local experimental alpha archive on native x86_64 GNU/Linux."""

import argparse
from datetime import datetime
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import subprocess
import stat
import tarfile
import tempfile

import package_windows_alpha as notice_helpers


ROOT = Path(__file__).resolve().parents[1]
SOURCE_FILES = {
    "README.md": ROOT / "docs/LINUX.md",
    "skills/binus-cli/SKILL.md": ROOT / "skills/binus-cli/SKILL.md",
    "LICENSE": ROOT / "LICENSE",
}
MEMBERS = ("binus", "README.md", "skills/binus-cli/SKILL.md", "LICENSE", "RUST-STDLIB-LICENSES.html", "THIRD-PARTY-NOTICES.txt")
CHECKSUM_MEMBERS = MEMBERS + ("BUILD-INFO.json",)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def require_regular_inputs(paths):
    result = {}
    for name, path in paths.items():
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"required input is missing, non-regular, or a symlink: {name}")
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        try:
            if not stat.S_ISREG(os.fstat(descriptor).st_mode):
                raise ValueError(f"required input is not a regular file: {name}")
            with os.fdopen(descriptor, "rb", closefd=False) as source:
                result[name] = source.read()
        finally:
            os.close(descriptor)
    return result


def validate_members(names):
    names = tuple(names)
    expected = set(MEMBERS) | {"BUILD-INFO.json", "SHA256SUMS"}
    if len(names) != len(set(names)) or set(names) != expected:
        raise ValueError("archive member allowlist mismatch")
    for name in names:
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or str(path) != name:
            raise ValueError("unsafe archive member name")
    return names


def archive_readme(data):
    """Adjust the bundled-skill link when the guide is placed at archive root."""
    source = b"[agent skill](../skills/binus-cli/SKILL.md)"
    target = b"[agent skill](skills/binus-cli/SKILL.md)"
    if data.count(source) != 1:
        raise ValueError("quick-start skill link did not match the expected source text")
    return data.replace(source, target)


def extract_archive(archive, destination):
    """Write only validated archive members into a new extraction directory."""
    with tarfile.open(archive, "r:gz") as tar:
        members = tar.getmembers()
        validate_members(member.name for member in members)
        contents = {}
        for member in members:
            if not member.isfile():
                raise ValueError("archive contains a non-regular member")
            stream = tar.extractfile(member)
            if stream is None:
                raise ValueError("archive member cannot be read")
            contents[member.name] = stream.read()
    destination = Path(destination)
    if destination.exists() and any(destination.iterdir()):
        raise ValueError("extraction directory must be empty")
    destination.mkdir(parents=True, exist_ok=True)
    for name in MEMBERS + ("BUILD-INFO.json", "SHA256SUMS"):
        target = destination.joinpath(*PurePosixPath(name).parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(target, flags, 0o600)
        with os.fdopen(descriptor, "wb") as output:
            output.write(contents[name])
        target.chmod(0o555 if name == "binus" else 0o444)


def create_exclusive(path, data):
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags, 0o644)
    with os.fdopen(descriptor, "wb") as output:
        output.write(data)


def commit_source_inputs(commit):
    result = {}
    for name, path in SOURCE_FILES.items():
        blob = run(["git", "show", f"{commit}:{path.relative_to(ROOT).as_posix()}"], cwd=ROOT).stdout.encode()
        current = require_regular_inputs({name: path})[name]
        if current != blob:
            raise SystemExit(f"source input differs from committed Git blob: {name}")
        result[name] = current
    return result


def verify_archive(archive, expected_data):
    with tarfile.open(archive, "r:gz") as tar:
        members = tar.getmembers()
        validate_members(member.name for member in members)
        extracted = {}
        for member in members:
            if not member.isfile():
                raise ValueError("archive contains a non-regular member")
            if member.mode != (0o555 if member.name == "binus" else 0o444):
                raise ValueError("archive member mode is unexpected")
            stream = tar.extractfile(member)
            if stream is None:
                raise ValueError("archive member cannot be read")
            extracted[member.name] = stream.read()
        for name, content in expected_data.items():
            if extracted.get(name) != content:
                raise ValueError(f"archive content mismatch: {name}")
        sums = extracted["SHA256SUMS"].decode("ascii")
        checksum_entries = {}
        for line in sums.splitlines():
            digest, name = line.split("  ", 1)
            if name not in CHECKSUM_MEMBERS:
                raise ValueError("checksum list contains an unexpected member")
            if name in checksum_entries:
                raise ValueError("checksum list contains a duplicate member")
            checksum_entries[name] = digest
            if sha256(extracted[name]) != digest:
                raise ValueError(f"archive checksum mismatch: {name}")
        if set(checksum_entries) != set(CHECKSUM_MEMBERS):
            raise ValueError("archive checksum list is incomplete")
        return extracted


def validate_elf_x86_64(data):
    if len(data) < 64 or data[:4] != b"\x7fELF" or data[4:6] != b"\x02\x01" or int.from_bytes(data[16:18], "little") not in (2, 3) or int.from_bytes(data[18:20], "little") != 62:
        raise ValueError("binary is not a complete little-endian ELF64 x86_64 executable")


def create_archive(path, data, timestamp):
    validate_members(data)
    with tarfile.open(path, "x:gz", format=tarfile.PAX_FORMAT) as tar:
        for name in MEMBERS + ("BUILD-INFO.json", "SHA256SUMS"):
            member = tarfile.TarInfo(name)
            member.size = len(data[name])
            member.mode = 0o555 if name == "binus" else 0o444
            member.mtime = int(datetime.fromisoformat(timestamp.replace("Z", "+00:00")).timestamp())
            tar.addfile(member, io.BytesIO(data[name]))


def audit_values():
    values = [b"auth_browser", b"binus-auth-debug"]
    for path in (Path.home(), ROOT):
        raw = str(path)
        for form in {raw, raw.replace("/", "\\"), raw.replace("\\", "/")}:
            values.extend((form.encode("utf-8"), form.encode("utf-16le")))
    return tuple(values)


def openssl_notice():
    candidates = (Path("/usr/share/doc/libssl3/copyright"), Path("/usr/share/doc/libssl3t64/copyright"))
    for path in candidates:
        if path.exists() or path.is_symlink():
            return str(path), notice_helpers.read_bounded_regular(path, 2 * 1024 * 1024)
    raise ValueError("required OpenSSL package copyright notice is unavailable")


def run(command, **kwargs):
    return subprocess.run(command, check=True, text=True, capture_output=True, **kwargs)


def compiler_environment(source_env=None):
    env = dict(os.environ if source_env is None else source_env)
    if "CARGO_ENCODED_RUSTFLAGS" in env or env.get("RUSTFLAGS"):
        raise RuntimeError("externally supplied Rust build flags are not allowed for provenance")
    env["RUSTFLAGS"] = f"--remap-path-prefix={Path.home()}=/build/home --remap-path-prefix={ROOT}=."
    return env


def host_facts():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("this pass supports native x86_64 GNU/Linux only")
    rustc = run(["rustc", "-vV"]).stdout
    match = re.search(r"(?m)^host: (\S+)$", rustc)
    if not match or match.group(1) != "x86_64-unknown-linux-gnu":
        raise RuntimeError("rustc host is not x86_64-unknown-linux-gnu")
    rustc_version = run(["rustc", "--version"]).stdout.strip()
    cargo_version = run(["cargo", "--version"]).stdout.strip()
    glibc_text = run(["ldd", "--version"]).stdout.splitlines()[0]
    return match.group(1), rustc_version, cargo_version, glibc_text


def dependency_facts(binary):
    ldd = run(["ldd", str(binary)]).stdout
    if "not found" in ldd:
        raise RuntimeError("binary has an unresolved dynamic dependency")
    sonames = sorted(set(re.findall(r"^\s*(lib[^\s]+\.so(?:\.[^\s]+)*)\s+=>", ldd, re.M)))
    readelf = run(["readelf", "--version-info", str(binary)]).stdout
    glibc = sorted(set(re.findall(r"GLIBC_(\d+(?:\.\d+)+)", readelf)), key=lambda v: tuple(map(int, v.split("."))))
    return {"dynamic_sonames": sonames, "highest_required_glibc_symbol": glibc[-1] if glibc else None}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True, help="existing Downloads folder outside this checkout")
    args = parser.parse_args()
    output_dir = args.output_dir.expanduser().resolve()
    if not output_dir.is_dir() or output_dir == ROOT or ROOT in output_dir.parents:
        raise SystemExit("output directory must already exist outside the repository")
    status = run(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT).stdout
    if status:
        raise SystemExit("source checkout is dirty; commit reviewed source before packaging")
    commit = run(["git", "rev-parse", "HEAD"], cwd=ROOT).stdout.strip()
    commit_source_inputs(commit)
    target, rustc_version, cargo_version, build_glibc = host_facts()
    archive = output_dir / f"alpha-{commit[:7]}-linux-x86_64.tar.gz"
    sidecar = output_dir / f"{archive.name}.sha256"
    if archive.exists() or sidecar.exists():
        raise SystemExit("refusing to overwrite an existing archive or checksum sidecar")

    with tempfile.TemporaryDirectory(prefix="binus-alpha-build-") as temp:
        temp_path = Path(temp)
        env = compiler_environment()
        stdlib_root = run(["rustc", "--print", "sysroot"]).stdout.strip()
        stdlib_notice = notice_helpers.rust_stdlib_notice(stdlib_root)
        third_party = notice_helpers.notice_text("x86_64-unknown-linux-gnu")
        openssl_name, openssl_text = openssl_notice()
        third_party += f"\n## System OpenSSL\n\nSource: Ubuntu package {openssl_name}.\n\n".encode("utf-8") + openssl_text
        build = run([
            "cargo", "build", "--locked", "--release", "--manifest-path",
            "spikes/001-auth/Cargo.toml", "--bin", "binus", "--target", target,
            "--message-format=json-render-diagnostics",
        ], cwd=ROOT, env=env)
        executable = None
        for line in build.stdout.splitlines():
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == "binus":
                candidate = event.get("executable")
                if candidate and Path(candidate).is_file():
                    executable = Path(candidate)
        if executable is None:
            raise RuntimeError("Cargo did not report the normal binus executable artifact")
        binary = require_regular_inputs({"binus": executable})["binus"]
        validate_elf_x86_64(binary)
        facts = dependency_facts(executable)
        if not any(name.startswith("libssl.so") for name in facts["dynamic_sonames"]):
            raise RuntimeError("the Linux package is expected to dynamically link OpenSSL libssl")
        if any(value and value in binary for value in audit_values()):
            raise RuntimeError("binary contains a prohibited diagnostic marker or unremapped local path")
        data = {"binus": binary}
        data.update(commit_source_inputs(commit))
        data["README.md"] = archive_readme(data["README.md"])
        data["RUST-STDLIB-LICENSES.html"] = stdlib_notice
        data["THIRD-PARTY-NOTICES.txt"] = third_party
        timestamp = run(["git", "show", "-s", "--format=%cI", commit], cwd=ROOT).stdout.strip()
        info = {
            "source_commit": commit, "native_target": target,
            "rustc": rustc_version, "cargo": cargo_version,
            "platform": platform.platform(), "build_glibc": build_glibc,
            **facts, "binary_sha256": sha256(data["binus"]),
            "timestamp_source": "source commit committer timestamp", "timestamp": timestamp,
            "compatibility": "Observed x86_64 GNU/Linux build environment and listed dynamic dependencies; not a universal or musl binary.",
        }
        data["BUILD-INFO.json"] = (json.dumps(info, indent=2, sort_keys=True) + "\n").encode()
        data["SHA256SUMS"] = "".join(f"{sha256(data[name])}  {name}\n" for name in CHECKSUM_MEMBERS).encode("ascii")
        validate_members(data)
        staged_archive = temp_path / archive.name
        create_archive(staged_archive, data, timestamp)
        extracted = verify_archive(staged_archive, data)
        scratch = temp_path / "extract"
        extract_archive(staged_archive, scratch)
        if extracted["RUST-STDLIB-LICENSES.html"] != stdlib_notice or len(stdlib_notice) > 2 * 1024 * 1024:
            raise ValueError("Rust standard library notice changed or exceeded its bound")
        isolated_home = temp_path / "help-home"
        isolated_home.mkdir()
        help_env = os.environ.copy()
        help_env.update({"HOME": str(isolated_home), "XDG_CONFIG_HOME": str(isolated_home / "config")})
        run([str(scratch / "binus"), "--help"], cwd=scratch, env=help_env)
        digest = sha256(staged_archive.read_bytes())
        status = run(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT).stdout
        if status or run(["git", "rev-parse", "HEAD"], cwd=ROOT).stdout.strip() != commit:
            raise SystemExit("source checkout changed during build; refusing to package")
        # Exclusive creation closes the pre-check/build race. Remove only paths created here.
        created = []
        try:
            with staged_archive.open("rb") as source:
                create_exclusive(archive, source.read())
            created.append(archive)
            if sha256(archive.read_bytes()) != digest:
                raise RuntimeError("published archive digest does not match staged archive")
            create_exclusive(sidecar, f"{digest}  {archive.name}\n".encode("ascii"))
            created.append(sidecar)
            if sidecar.read_text(encoding="ascii") != f"{digest}  {archive.name}\n":
                raise RuntimeError("published checksum sidecar does not match staged archive")
        except Exception:
            for path in created:
                path.unlink(missing_ok=True)
            raise
        print(f"Verified archive: {archive}")
        print(f"Archive SHA-256: {sidecar}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError, tarfile.TarError) as exc:
        raise SystemExit(f"packaging failed: {exc}") from None
