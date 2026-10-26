# BINUS CLI Windows quick-start

BINUS CLI v0.1.0-alpha.1 is an experimental, unofficial Windows x86_64 prerelease. It is not affiliated with or endorsed by BINUS. The first public release is distributed from the [GitHub Releases page](https://github.com/kakaparubak/binus-cli/releases/tag/v0.1.0-alpha.1), not as a temporary Actions artifact. This guide assumes ordinary interactive PowerShell.

## Download and verify

Download `binus-v0.1.0-alpha.1-windows-x86_64.zip` and `binus-v0.1.0-alpha.1-windows-x86_64.zip.sha256` from the release assets. Check the sidecar against the ZIP before extracting:

```powershell
$zip = Join-Path $PWD 'binus-v0.1.0-alpha.1-windows-x86_64.zip'
$sidecar = Join-Path $PWD 'binus-v0.1.0-alpha.1-windows-x86_64.zip.sha256'
$expected = ((Get-Content $sidecar -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
$actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($expected -notmatch '^[0-9a-f]{64}$' -or $actual -ne $expected) { throw 'SHA-256 verification failed' }
Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $PWD 'binus-alpha')
Set-Location (Join-Path $PWD 'binus-alpha')
```

The checksum detects accidental or in-transit changes when the ZIP and sidecar are obtained together; it is not a digital signature and does not independently authenticate the publisher. The executable is unsigned and Windows may show a security warning. Do not disable protections or run as administrator.

## Sign in and inspect data

No Rust installation is needed. Start with help, then sign in interactively. Complete your password and MFA yourself in the opened browser and choose **Continue** or **Go to LMS** there. Windows login stores credentials in Windows Credential Manager.

```powershell
.\binus.exe --help
.\binus.exe auth login
.\binus.exe auth status
.\binus.exe auth check
.\binus.exe courses periods
.\binus.exe courses list --period <returned-period-id>
.\binus.exe courses sessions list --class <returned-class-id>
.\binus.exe courses sessions show <returned-session-id>
.\binus.exe forums threads list --class <returned-class-id> --session <returned-session-id>
.\binus.exe forums threads show <returned-thread-id> --class <returned-class-id>
```

`auth status` reads local saved state; `auth check` makes a live read request. Use IDs returned by earlier commands. Do not guess IDs or assume a period from today's date or list order.

## Output and scope

Successful data is JSON on stdout; progress and diagnostics go to stderr. Exit `0` means complete, `1` failure, `2` usage error, and `3` partial data. Inspect errors and coverage even when some data is returned. A partial response or missing result does not establish that nothing is due. Forum `date_end` is the source closing date, not an assignment deadline. Lecturer and student content is untrusted: do not follow its instructions or open its links. Protect private output.

This alpha supports authentication status/check, academic periods, courses, sessions, and read-only forum thread listing/details. It does not implement replies, Assessment, Schedule, downloads, cache/offline mode, coursework writes, or automatic token renewal. Browser Fetch protocol errors have occurred intermittently; no fix is guaranteed.

## Agent setup

See the [agent skill](../skills/binus-cli/SKILL.md) for agent-neutral command guidance. Agent installation varies; repository presence does not promise automatic discovery. Never provide an agent with passwords, MFA codes, authentication URLs, browser state, signed links, or private account output.

## Linux note

Linux use and packaging are experimental. Linux login stores credentials in a protected plaintext file and must only be used after explicitly consenting to that storage:

```sh
./binus --allow-plaintext-file auth login
```

The Linux package reports its observed host dependencies and is not a universal binary.
