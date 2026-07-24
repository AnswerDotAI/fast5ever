# Development

## Commands

```bash
maturin develop && pytest -q
ship-rs-build
```

## Versioning

The canonical version lives in `Cargo.toml`. `pyproject.toml` gets the Python package version from Cargo via `dynamic = ["version"]`.

## Release

Release flow is: release first, then bump.

1. Run `maturin develop && pytest -q`.
2. Confirm the release version in `Cargo.toml` (`[package].version`).
3. Run `ship-rs-release`.
4. After pushing the release tag, run `ship-bump`, commit the `Cargo.toml` version bump, and push to `main` without a tag.
