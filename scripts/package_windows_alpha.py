#!/usr/bin/env python3
"""Build and verify an experimental native x86_64 Windows alpha ZIP."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import stat
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MEMBERS = ("binus.exe", "README.md", "skills/binus-cli/SKILL.md", "LICENSE", "RUST-STDLIB-LICENSES.html", "BUILD-INFO.json", "THIRD-PARTY-NOTICES.txt", "SHA256SUMS")
CHECKSUM_MEMBERS = MEMBERS[:-1]
DOCS = {"README.md": "docs/QUICKSTART.md", "skills/binus-cli/SKILL.md": "skills/binus-cli/SKILL.md", "LICENSE": "LICENSE"}
INBOX_DLLS = {"advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "cfgmgr32.dll", "combase.dll", "crypt32.dll", "gdi32.dll", "imm32.dll", "iphlpapi.dll", "kernel32.dll", "kernelbase.dll", "msvcrt.dll", "ntdll.dll", "ole32.dll", "oleaut32.dll", "normaliz.dll", "psapi.dll", "rpcrt4.dll", "secur32.dll", "shell32.dll", "shlwapi.dll", "ucrtbase.dll", "user32.dll", "userenv.dll", "version.dll", "winhttp.dll", "wininet.dll", "ws2_32.dll", "wldap32.dll"}
MARKERS = (b"auth_browser", b"binus-auth-debug")


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def validate_members(names):
    names = tuple(names)
    if len(names) != len(set(names)) or set(names) != set(MEMBERS):
        raise ValueError("ZIP member allowlist mismatch")
    for name in names:
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or str(path) != name or "\\" in name:
            raise ValueError("unsafe ZIP member name")
    return names


def archive_readme(data):
    source, target = b"[agent skill](../skills/binus-cli/SKILL.md)", b"[agent skill](skills/binus-cli/SKILL.md)"
    if data.count(source) != 1:
        raise ValueError("quick-start skill link did not match expected source text")
    return data.replace(source, target)


def parse_pe_x64(data):
    def need(offset, size):
        if offset < 0 or size < 0 or offset + size > len(data):
            raise ValueError("truncated PE executable")
    need(0, 64)
    if data[:2] != b"MZ":
        raise ValueError("invalid PE DOS signature")
    pe = int.from_bytes(data[0x3c:0x40], "little")
    need(pe, 24)
    if data[pe:pe + 4] != b"PE\0\0" or int.from_bytes(data[pe + 4:pe + 6], "little") != 0x8664:
        raise ValueError("executable is not x86_64 PE")
    sections = int.from_bytes(data[pe + 6:pe + 8], "little")
    optional_size = int.from_bytes(data[pe + 20:pe + 22], "little")
    optional = pe + 24
    need(optional, optional_size)
    if optional_size < 112 or int.from_bytes(data[optional:optional + 2], "little") != 0x20b:
        raise ValueError("invalid PE32+ optional header")
    dir_count = int.from_bytes(data[optional + 108:optional + 112], "little")
    imports = []
    import_rva = import_size = 0
    if dir_count > 1 and optional_size >= 128:
        import_rva, import_size = int.from_bytes(data[optional + 120:optional + 124], "little"), int.from_bytes(data[optional + 124:optional + 128], "little")
    if bool(import_rva) != bool(import_size):
        raise ValueError("invalid PE import directory bounds")
    table = optional + optional_size
    need(table, sections * 40)
    ranges = []
    for i in range(sections):
        off = table + i * 40
        virtual_size = int.from_bytes(data[off + 8:off + 12], "little")
        virtual_address = int.from_bytes(data[off + 12:off + 16], "little")
        raw_size = int.from_bytes(data[off + 16:off + 20], "little")
        raw_ptr = int.from_bytes(data[off + 20:off + 24], "little")
        need(raw_ptr, raw_size)
        ranges.append((virtual_address, virtual_size, raw_size, raw_ptr))
    def section_for(rva):
        for va, _virtual_size, raw_size, raw in ranges:
            if va <= rva < va + raw_size:
                pos = raw + rva - va
                need(pos, 1)
                return va, raw_size, raw, pos
        raise ValueError("PE import RVA is outside sections")
    if import_rva and import_size:
        if import_size < 20:
            raise ValueError("invalid PE import directory bounds")
        section_va, section_raw_size, section_raw, pos = section_for(import_rva)
        section_end = min(len(data), section_raw + section_raw_size)
        count = min(import_size // 20, 512)
        terminated = False
        for index in range(count):
            pos = section_raw + import_rva - section_va + index * 20
            if pos + 20 > section_end:
                raise ValueError("PE import table exceeds bounded descriptor count")
            need(pos, 20)
            desc = data[pos:pos + 20]
            if not any(desc):
                terminated = True
                break
            name_rva = int.from_bytes(desc[12:16], "little")
            name_va, name_raw_size, name_raw, name_pos = section_for(name_rva)
            name_limit = min(len(data), name_pos + 260, name_raw + name_raw_size)
            end = data.find(b"\0", name_pos, name_limit)
            if end < 0:
                raise ValueError("unterminated PE import name")
            try:
                name = data[name_pos:end].decode("ascii").lower()
            except UnicodeDecodeError:
                raise ValueError("invalid non-ASCII PE import name") from None
            if not name or len(name) > 259 or not name.endswith(".dll") or not re.fullmatch(r"[a-z0-9_.+-]+", name):
                raise ValueError("invalid PE import DLL name")
            imports.append(name)
            pos += 20
        if not terminated:
            raise ValueError("PE import table exceeds bounded descriptor count")
    return sorted(set(imports))


def verify_zip(archive, expected=None):
    with zipfile.ZipFile(archive) as zf:
        infos = zf.infolist()
        validate_members(info.filename for info in infos)
        contents = {}
        for info in infos:
            mode = (info.external_attr >> 16) & 0xFFFF
            if not stat.S_ISREG(mode):
                raise ValueError("ZIP contains a non-regular member")
            if info.is_dir():
                raise ValueError("ZIP contains a directory entry")
            contents[info.filename] = zf.read(info)
    if expected is not None:
        for name, data in expected.items():
            if contents.get(name) != data:
                raise ValueError(f"archive content mismatch: {name}")
    try:
        sums = contents["SHA256SUMS"].decode("ascii")
        entries = {}
        for line in sums.splitlines():
            digest, name = line.split("  ", 1)
            if name not in CHECKSUM_MEMBERS or name in entries:
                raise ValueError("checksum list has an unexpected or duplicate member")
            entries[name] = digest
            if sha256(contents[name]) != digest:
                raise ValueError(f"archive checksum mismatch: {name}")
        if set(entries) != set(CHECKSUM_MEMBERS):
            raise ValueError("checksum list is incomplete")
    except (KeyError, UnicodeError, ValueError) as exc:
        if isinstance(exc, ValueError) and str(exc).startswith(("archive checksum", "checksum list")):
            raise
        raise ValueError("invalid SHA256SUMS") from None
    return contents


def write_zip_contents(zf, contents):
    """Write allowlisted files with explicit Unix regular-file metadata."""
    for name in MEMBERS:
        info = zipfile.ZipInfo(name)
        info.create_system = 3
        permissions = 0o755 if name == "binus.exe" else 0o644
        info.external_attr = (stat.S_IFREG | permissions) << 16
        info.compress_type = zf.compression
        if zf.compresslevel is not None:
            info._compresslevel = zf.compresslevel
        zf.writestr(info, contents[name])


def extract_zip(archive, destination):
    contents = verify_zip(archive)
    destination = Path(destination)
    if destination.exists() and any(destination.iterdir()):
        raise ValueError("extraction directory must be empty")
    destination.mkdir(parents=True, exist_ok=True)
    for name in MEMBERS:
        target = destination.joinpath(*PurePosixPath(name).parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as output:
            output.write(contents[name])
    return contents


def require_regular(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError("required input is missing, non-regular, or a symlink")
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    fd = os.open(path, flags)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise ValueError("input is not a regular file")
        with os.fdopen(fd, "rb", closefd=False) as f:
            return f.read()
    finally:
        os.close(fd)


def read_bounded_regular(path, limit):
    """Read a required non-symlink regular file, refusing oversized input."""
    path = Path(path)
    if path.is_symlink():
        raise ValueError("required notice must be a regular non-symlink file")
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    try:
        fd = os.open(path, flags)
    except OSError as exc:
        raise ValueError("required notice is missing or not a regular file") from exc
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise ValueError("required notice must be a regular non-symlink file")
        with os.fdopen(fd, "rb", closefd=False) as source:
            data = source.read(limit + 1)
        if len(data) > limit:
            raise ValueError("required notice exceeds bounded size")
        return data
    finally:
        os.close(fd)


def rust_stdlib_notice(sysroot):
    return read_bounded_regular(Path(sysroot) / "share/doc/rust/COPYRIGHT-library.html", 2 * 1024 * 1024)


def create_exclusive(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o644)
    with os.fdopen(fd, "wb") as output:
        output.write(data)


def bounded_notice_files(root, license_file=None):
    root = Path(root).resolve()
    paths = [p for p in root.iterdir() if p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE"))]
    if license_file:
        paths.append(root / license_file)
    result = {}
    for path in paths:
        if path.is_symlink():
            if license_file and path == root / license_file:
                raise ValueError("license-file must be a regular non-symlink file")
            continue
        resolved = path.resolve()
        if root not in resolved.parents:
            raise ValueError("dependency notice path escapes package root")
        if not path.is_file():
            if license_file and path == root / license_file:
                raise ValueError("license-file must be a regular non-symlink file")
            continue
        size = path.stat().st_size
        if size > 256 * 1024:
            if license_file and path == root / license_file:
                raise ValueError("license-file exceeds bounded size")
            continue
        result[path.name] = path.read_text(encoding="utf-8", errors="replace")
    return result


def run(cmd, **kwargs):
    return subprocess.run(cmd, check=True, text=True, capture_output=True, **kwargs)


def git_blob(commit, path):
    return run(["git", "show", f"{commit}:{path}"], cwd=ROOT).stdout.encode("utf-8")


def normalized_doc_bytes(data):
    return data.replace(b"\r\n", b"\n")


def verify_worktree_doc(worktree, committed, path):
    if normalized_doc_bytes(worktree) != normalized_doc_bytes(committed):
        raise SystemExit(f"source input differs from committed Git blob: {path}")


def audit_values():
    values = [*MARKERS]
    for path in (Path.home(), ROOT):
        raw = str(path)
        forms = {raw, raw.replace("/", "\\"), raw.replace("\\", "/")}
        for form in forms:
            encoded = form.encode("utf-8")
            values.extend((encoded, encoded.decode("utf-8").encode("utf-16le")))
    return tuple(values)


def non_inbox_imports(imports):
    return sorted(name for name in set(imports) - INBOX_DLLS if not name.startswith(("api-ms-win-", "ext-ms-win-")))


def notice_text():
    metadata = json.loads(run(["cargo", "metadata", "--locked", "--filter-platform", "x86_64-pc-windows-msvc", "--manifest-path", "spikes/001-auth/Cargo.toml", "--format-version", "1"]).stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = metadata.get("workspace_default_members") or metadata["workspace_members"]
    reachable, pending = set(), list(roots)
    while pending:
        package_id = pending.pop()
        if package_id in reachable:
            continue
        reachable.add(package_id)
        pending.extend(dep["pkg"] for dep in nodes.get(package_id, {}).get("deps", []))
    lines = ["Third-party dependency license materials", "", "Based on Cargo metadata --locked --filter-platform x86_64-pc-windows-msvc and its reachable resolve graph. License metadata and included texts are informational; legal review is pending.", ""]
    missing_text = []
    for package_id in sorted(reachable):
        package = packages[package_id]
        if package.get("source") is None:
            continue
        root = Path(package["manifest_path"]).parent.resolve()
        lines.append(f"## {package['name']} {package['version']} — license metadata: {package.get('license') or 'unspecified'}")
        license_file = package.get("license_file")
        relative = None
        if license_file:
            license_path = Path(license_file)
            relative = str(license_path.relative_to(root)) if license_path.is_absolute() and root in license_path.resolve().parents else str(license_path)
        files = bounded_notice_files(root, relative)
        if not files:
            lines.append("No public license or notice text was available in the bounded package-root scan.")
            missing_text.append(f"{package['name']} {package['version']}")
        for filename, content in sorted(files.items()):
            lines += [f"\n### {filename}\n", content.rstrip()]
        lines.append("")
    rustc = run(["rustc", "--print", "sysroot"]).stdout.strip()
    toolchain_root = Path(rustc)
    lines += ["## Rust standard library", "", "The standard library and associated third-party license notices are included verbatim in RUST-STDLIB-LICENSES.html."]
    lines += ["", "Unavailable package license texts: " + (", ".join(missing_text) if missing_text else "none detected by the bounded scan") + ".", "Project license: MIT; see LICENSE. Dependencies retain their respective licenses. This notice does not change ownership of third-party code."]
    return ("\n".join(lines).rstrip() + "\n").encode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "nt" or platform.machine().lower() not in ("amd64", "x86_64"):
        raise SystemExit("native Windows x86_64 build required")
    out = args.output_dir.resolve()
    if not out.is_dir() or out == ROOT or ROOT in out.parents:
        raise SystemExit("output directory must exist outside checkout")
    status = run(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT).stdout
    if status:
        raise SystemExit("source checkout is dirty")
    commit = run(["git", "rev-parse", "HEAD"], cwd=ROOT).stdout.strip()
    target, rustc = "x86_64-pc-windows-msvc", run(["rustc", "-vV"]).stdout
    if f"host: {target}" not in rustc or not run(["rustc", "--version"]).stdout.startswith("rustc 1.99.1 "):
        raise SystemExit("expected native x86_64 MSVC Rust 1.99.1 toolchain")
    archive = out / f"binus-{commit[:7]}-windows-x86_64.zip"
    sidecar = Path(str(archive) + ".sha256")
    if archive.exists() or sidecar.exists():
        raise SystemExit("refusing to overwrite existing output")
    with tempfile.TemporaryDirectory(prefix="binus-windows-alpha-") as td:
        temp = Path(td)
        env = dict(os.environ)
        flags = [f"--remap-path-prefix={Path.home()}=/build/home", f"--remap-path-prefix={ROOT}=.", "-C", "target-feature=+crt-static"]
        env.pop("RUSTFLAGS", None)
        env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
        doc_blobs = {}
        for name, path in DOCS.items():
            blob = git_blob(commit, path)
            verify_worktree_doc(require_regular(ROOT / path), blob, path)
            doc_blobs[name] = blob
        stdlib_notice = rust_stdlib_notice(run(["rustc", "--print", "sysroot"]).stdout.strip())
        build = run(["cargo", "build", "--locked", "--release", "--manifest-path", "spikes/001-auth/Cargo.toml", "--bin", "binus", "--target", target, "--message-format=json-render-diagnostics"], cwd=ROOT, env=env)
        exe = None
        for line in build.stdout.splitlines():
            try: event = json.loads(line)
            except json.JSONDecodeError: continue
            if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == "binus" and event.get("executable"):
                exe = Path(event["executable"])
        if not exe or not exe.is_file(): raise RuntimeError("Cargo did not report binus.exe compiler artifact")
        binary = require_regular(exe)
        imports = parse_pe_x64(binary)
        missing = non_inbox_imports(imports)
        if missing: raise RuntimeError("non-inbox runtime imports require onboarding review: " + ", ".join(missing))
        if any(value in binary for value in audit_values()):
            raise RuntimeError("binary contains a prohibited diagnostic marker or unremapped local path")
        data = {"binus.exe": binary}
        for name, blob in doc_blobs.items():
            data[name] = archive_readme(blob) if name == "README.md" else blob
        data["RUST-STDLIB-LICENSES.html"] = stdlib_notice
        data["THIRD-PARTY-NOTICES.txt"] = notice_text()
        info = {"source_commit": commit, "native_target": target, "rustc": run(["rustc", "--version" ]).stdout.strip(), "cargo": run(["cargo", "--version"]).stdout.strip(), "platform": platform.platform(), "runner_os": "Windows Server 2025", "evaluation_target": "Windows 11 x86_64 (previous manual-account evaluation evidence)", "validation_scope": "native Windows Server 2025 build and extracted --help only; no Windows 11 observation in this CI run", "crt_static": True, "imports": imports, "binary_sha256": sha256(binary), "compatibility": "Windows 11 x86_64 is the evaluation target; universal compatibility is not claimed."}
        data["BUILD-INFO.json"] = (json.dumps(info, indent=2, sort_keys=True) + "\n").encode()
        data["SHA256SUMS"] = "".join(f"{sha256(data[n])}  {n}\n" for n in CHECKSUM_MEMBERS).encode("ascii")
        staged = temp / archive.name
        with zipfile.ZipFile(staged, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zf:
            write_zip_contents(zf, data)
        verify_zip(staged, data)
        extracted = temp / "extract"
        extract_zip(staged, extracted)
        for name, path in DOCS.items():
            verify_worktree_doc(require_regular(ROOT / path), git_blob(commit, path), path)
        isolated = temp / "isolated"
        isolated.mkdir()
        help_env = dict(os.environ)
        for key in ("HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"):
            help_env[key] = str(isolated)
        run([str(extracted / "binus.exe"), "--help"], cwd=extracted, env=help_env)
        digest = sha256(staged.read_bytes())
        if run(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT).stdout or run(["git", "rev-parse", "HEAD"], cwd=ROOT).stdout.strip() != commit:
            raise SystemExit("source checkout changed during build")
        created = []
        try:
            create_exclusive(archive, staged.read_bytes())
            created.append(archive)
            if sha256(archive.read_bytes()) != digest: raise RuntimeError("archive readback digest mismatch")
            create_exclusive(sidecar, f"{digest}  {archive.name}\n".encode())
            created.append(sidecar)
            if sidecar.read_text(encoding="ascii") != f"{digest}  {archive.name}\n": raise RuntimeError("sidecar readback mismatch")
        except Exception:
            for path in created: path.unlink(missing_ok=True)
            raise
        print(f"Verified archive: {archive}")
        print(f"SHA-256 sidecar: {sidecar}")


if __name__ == "__main__":
    try: main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError, zipfile.BadZipFile) as exc:
        raise SystemExit(f"packaging failed: {exc}") from None
