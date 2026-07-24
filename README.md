# fast5ever

PyO3/maturin package scaffolded by fastship.

## Development

```bash
pip install -e .[dev]
maturin develop && pytest -q
```

## Build

```bash
ship-rs-build
```

## Release

Release flow is: release first, then bump.

```bash
maturin develop && pytest -q
ship-rs-release
ship-bump
```

The GitHub workflow builds wheels on tags matching `v*` and publishes them to GitHub Releases and PyPI.
