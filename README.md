# BINUS CLI experiments

This repository contains a bounded authentication feasibility spike and an incremental read-only Courses discovery slice. See [spikes/001-auth/README.md](spikes/001-auth/README.md) for auth boundaries. Run course commands with `cargo run --manifest-path spikes/001-auth/Cargo.toml --bin binus -- courses periods` or `... -- courses list --period <catalog-id>`; period selection is explicit.
