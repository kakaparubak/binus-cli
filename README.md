# BINUS CLI

BINUS CLI is an experimental, unofficial client for BINUS University services. It is not affiliated with or endorsed by BINUS. It provides JSON-first, read-only access for people and shell agents.

**Windows prerelease:** [v0.1.0-alpha.1](https://github.com/kakaparubak/binus-cli/releases/tag/v0.1.0-alpha.1) · [Quick-start guide](docs/QUICKSTART.md)

| Area | Available | Not implemented |
| --- | --- | --- |
| Authentication | Human interactive login, local status, live check | Automatic token renewal |
| Courses | Academic periods and course lists | Schedule, Assessment |
| Sessions | Lists and session details | Downloads, cache/offline mode |
| Forums | Thread lists and details | Replies and other writes |

The Windows x86_64 alpha ZIP includes a native executable; Rust is not needed to run it. The guide covers SHA-256 verification, extraction, and first commands. The project is licensed under [MIT](LICENSE); third-party licenses are listed separately in the archive.

For agent-neutral setup guidance, see [skills/binus-cli/SKILL.md](skills/binus-cli/SKILL.md). Agent installation varies; repository presence does not imply automatic discovery.

## Quick example

```powershell
.\binus.exe auth status
.\binus.exe auth check
.\binus.exe courses periods
.\binus.exe courses list --period <returned-period-id>
```

Successful data is JSON on stdout; progress and diagnostics go to stderr. A partial response does not prove that nothing is due.

| Exit code | Meaning |
| --- | --- |
| `0` | Complete |
| `1` | Failure |
| `2` | Usage error |
| `3` | Partial data |

See the [quick-start guide](docs/QUICKSTART.md) for login, credential handling, command examples, and limitations.

## Security and limitations

Complete sign-in and MFA yourself in the browser. Windows credentials use Windows Credential Manager. Do not elevate the program or disable operating-system warnings; this unsigned prerelease may show them. Protect private output and never share passwords, MFA codes, authentication URLs, browser state, or signed links. Browser Fetch protocol errors have occurred intermittently and remain unresolved.

## Build and contribute

For development, build and test the actual manifest and binary with `cargo build --locked --release --manifest-path spikes/001-auth/Cargo.toml --bin binus` and run `python -m unittest discover -s tests`. After a Windows release build, the Python packaging suite can be run with `python -m unittest tests.test_windows_alpha`.

Linux source builds are experimental; Linux login requires explicit consent to plaintext credential storage. The Linux package describes its observed host requirements and is not a universal binary. Contributions and bug reports are welcome through GitHub issues. Never include credentials, private account data, or live-service output in issues or fixtures; use synthetic public test data.
