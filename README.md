# fast5ever

WHATWG-compliant HTML parsing, mutation, and serialization for Python, powered by Rust's [html5ever](https://github.com/servo/html5ever) (the engine written for Servo).

html5ever uses the spec's algorithms but does not include the tree. fast5ever adds the missing piece, a fast arena DOM, and exposes both through a small Python API. Parsing, error recovery, and serialization all behave exactly as a browser's `innerHTML` does, because they are the same algorithms.

```python
from fast5ever import parse, parse_fragment

frag = parse_fragment('<p>one<p>two')
frag.to_html()                        # '<p>one</p><p>two</p>'
[c.name for c in frag.children]       # ['p', 'p']

doc = parse('<!DOCTYPE html><title>t</title>hello')
doc.to_html()                         # '<!DOCTYPE html><html><head><title>t'...
```

## API

- `parse(html)` parses a complete document; `parse_fragment(html, context='body')` parses a fragment in a context element (pass e.g. `context='tbody'` to parse table rows). Both return the `#document` node.
- `Node` is a lightweight handle: `.name` (tag, or `#document`/`#text`/`#comment`/`#doctype`), `.attrs` (dict snapshot in source order), `.text` (own content for text/comments), `.children`, `.parent`, `to_html()`, `to_text()`.
- Mutation: `set_attr`/`del_attr`/`get_attr`, `append_child`, `insert_before(child, reference)`, `replace_child(new, old)`, `detach()`. Inserting a `#document` node splices its children in (DocumentFragment semantics), so `div.replace_child(parse_fragment(markup), old)` splices markup in place. Inserting a node from another tree deep-copies it; handles stay valid across all mutations.

## Serialization is the spec's

Output spelling comes from html5ever's own serializer - the WHATWG serialization algorithm, byte-for-byte what Chrome's `innerHTML` builds: boolean attributes as `open=""`, double-quoted values, voids without `/`, raw text unescaped inside `script`/`style`. fast5ever adds no styling options and no compatibility shims with other serializers, by design.

Parsing is also bounded Chromium-style: element nesting beyond 512 flattens at the cap (Chromium's own limit), which keeps adversarially deep input linear-time - html5ever's tree builder alone is quadratic there.

## Development

```bash
pip install -e .[dev]
maturin develop && pytest -q
```

All tests are pytest; `cargo check`/`cargo clippy` stay warning-free and need no Python (pyo3 sits behind the `python` feature).

## Release

Release flow is: release first, then bump.

```bash
maturin develop && pytest -q
ship-rs-release
ship-bump
```

The GitHub workflow builds wheels on tags matching `v*` and publishes them to GitHub Releases and PyPI.
