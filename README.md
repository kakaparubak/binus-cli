# BINUS CLI alpha

The native CLI supports authentication status and checks, academic period and course discovery, session listing and details, and read-only forum thread listing and details. See the [agent-neutral BINUS CLI skill](skills/binus-cli/SKILL.md) for safe setup, command use, and output interpretation. This alpha does not support coursework submissions or downloads.

For the human [quick-start](docs/QUICKSTART.md) and platform-specific login guidance, see the guide. The packager requires a clean source checkout (its script and inputs are not included in the native-only archive). After committing the packaging source, run `python3 scripts/package_alpha.py --output-dir "$HOME/Downloads"` from the checkout to build and verify a native x86_64 GNU/Linux archive outside the repository.
