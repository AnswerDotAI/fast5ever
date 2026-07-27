# Development

## Commands

```bash
maturin develop && pytest -q
ship-rs-build
```

## Versioning

The canonical version lives in `Cargo.toml`. `pyproject.toml` gets the Python package version from Cargo via `dynamic = ["version"]`.

## Release

Release flow is: release first, then bump - `ship-release` does both.

1. Run `maturin develop && pytest -q`.
2. Confirm the release version in `Cargo.toml` (`[package].version`).
3. Run `ship-release`. It tags `v<version>`, pushes branch and tag (CI builds and publishes), then bumps `Cargo.toml`, refreshes the editable install, and pushes the bump without a tag.
