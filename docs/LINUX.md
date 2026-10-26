# BINUS CLI Linux quick-start

BINUS CLI v0.1.0-alpha.1 is an experimental, unofficial native x86_64 GNU/Linux prerelease. It is not affiliated with or endorsed by BINUS. The Linux archive is built from a packaging-only source commit; its `BUILD-INFO.json` records that source commit, while application source and the existing v0.1.0-alpha.1 tag remain unchanged. Native CI validates the Ubuntu 24.04 build host and extracted `--help`; it does not test a human login or prove GUI behavior on other distributions.

## Download and verify

Download `binus-v0.1.0-alpha.1-linux-x86_64.tar.gz` and `binus-v0.1.0-alpha.1-linux-x86_64.tar.gz.sha256` from the [v0.1.0-alpha.1 release](https://github.com/kakaparubak/binus-cli/releases/tag/v0.1.0-alpha.1). In the directory containing both files:

```sh
sha256sum -c binus-v0.1.0-alpha.1-linux-x86_64.tar.gz.sha256
mkdir binus-alpha
tar -xzf binus-v0.1.0-alpha.1-linux-x86_64.tar.gz -C binus-alpha
cd binus-alpha
./binus --help
```

The checksum detects accidental or in-transit changes when the archive and sidecar are obtained together; it is not a digital signature and does not independently authenticate the publisher. The native package records the measured GLIBC symbol requirement and dynamic dependencies for its build host. Ubuntu 24.04 normally uses GLIBC 2.39 and OpenSSL 3, but compatibility depends on the producer's recorded measurements and the host's available libraries; universal Linux compatibility is not claimed.

## Sign in and inspect data

Login requires a human interactive Wayland desktop session and Chromium. In that session, explicitly consent to storing credentials in a protected plaintext file:

```sh
./binus --allow-plaintext-file auth login
```

Credentials are not stored in a system keyring. Complete password and MFA entry yourself in the browser and finish the requested LMS step there. CI only runs `--help` with an isolated home; it does not invoke authentication, access a live service, or establish headless GUI behavior.

Use IDs returned by earlier commands. The CLI does not infer a semester or due status from dates, list order, or missing results.

```sh
./binus auth status
./binus auth check
./binus courses periods
./binus courses list --period <returned-period-id>
./binus courses sessions list --class <returned-class-id>
./binus courses sessions show <returned-session-id>
./binus forums threads list --class <returned-class-id> --session <returned-session-id>
./binus forums threads show <returned-thread-id> --class <returned-class-id>
```

Successful data is JSON on stdout; progress and diagnostics go to stderr. Exit `0` means complete, `1` failure, `2` usage error, and `3` partial data. Inspect errors and coverage even when some data is returned. A partial response or missing result does not establish that nothing is due.

This alpha supports authentication status/check, academic periods, courses, sessions, and read-only forum thread listing/details. It does not implement replies or other writes, Assessment, Schedule, downloads, cache/offline mode, or automatic token renewal. Protect private output and never share passwords, MFA codes, authentication URLs, browser state, signed links, or account data.

## Agent setup

See the [agent skill](../skills/binus-cli/SKILL.md) for agent-neutral command guidance. Agent installation varies; repository presence does not promise automatic discovery.
