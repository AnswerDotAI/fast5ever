# fast5ever

WHATWG-compliant HTML parsing, mutation, and serialization for Python, powered by Rust's [html5ever](https://github.com/servo/html5ever) (the engine written for Servo).

html5ever uses the spec's algorithms but does not include the tree. fast5ever adds the missing piece, a fast arena DOM, and exposes both through a small Python API. Parsing, error recovery, and serialization all behave exactly as a browser's `innerHTML` does, because they are the same algorithms.

```python
from fast5ever import parse, parse_fragment

frag = parse_fragment('<p>one<p>two')
frag.to_html()                        # '<p>one</p><p>two</p>'
[c.name for c in frag.children]       # ['p', 'p']
frag.children[0].attrs['class'] = 'lead'   # attrs is live: writes go straight to the tree

doc = parse('<!DOCTYPE html><title>t</title>hello')
doc.to_html()                         # '<!DOCTYPE html><html><head><title>t'...
```

## API

- `parse(html)` parses a complete document; `parse_fragment(html, context='body')` parses a fragment in a context element (pass e.g. `context='tbody'` to parse table rows). Both return a `Document` node.
- Every node is a `Document`, `Element`, `Text`, `Comment`, or `Doctype` - all subclasses of `Node` - so kind checks are `isinstance(c, Text)`. Shared surface: `.name` (tag, or `#document`/`#text`/`#comment`/`#doctype`; writable on elements — `el.name = 'details'` renames in place, keeping attributes and children), `.children`, `.parent`, `to_html()`, `to_text()`.
- `el.attrs` is a live mapping in source order: `attrs['k']`, `attrs['k'] = v`, `del attrs['k']`, `in`/`len`/iteration, `get`/`keys`/`values`/`items`/`update`/`pop`, `== {...}` against any mapping; `dict(attrs)` snapshots. (Non-elements read as empty and refuse writes.)
- `.text` is a text or comment node's own content, writable: `t.text = 'new'`. `template.content` is a `<template>` element's contents as a `Document`.
- `Element(name, attrs=None)`, `Text(text)`, and `Comment(text)` construct detached nodes to insert.
- Structure: `append_child`, `insert_before(child, reference)`, `replace_child(new, old)`, `detach()`. Inserting a `Document` node splices its children in (DocumentFragment semantics), so `div.replace_child(parse_fragment(markup), old)` splices markup in place. Inserting a node from another tree deep-copies it; handles stay valid across all mutations.

The node API follows the WHATWG DOM's vocabulary, with a pythonic surface (real node classes, attrs as a live mapping, `to_html()`/`to_text()`) modeled on Emil Stenström's [JustHTML](https://github.com/EmilStenstrom/justhtml). The parsing and serialization engine is Servo's [html5ever](https://github.com/servo/html5ever).

## Serialization is the spec's

Output spelling comes from html5ever's own serializer - the WHATWG serialization algorithm, byte-for-byte what Chrome's `innerHTML` builds: boolean attributes as `open=""`, double-quoted values, voids without `/`, raw text unescaped inside `script`/`style`. fast5ever adds no styling options and no compatibility shims with other serializers, by design.

Parsing is also bounded Chromium-style: element nesting beyond 512 flattens at the cap (Chromium's own limit), which keeps adversarially deep input linear-time - html5ever's tree builder alone is quadratic there.

## The Rust API

fast5ever is a Rust crate first (`fast5ever = { git = "https://github.com/AnswerDotAI/fast5ever" }`); the Python API is a thin binding over it, so the two surfaces match verb for verb. The one structural difference: Rust has no node objects - you hold the `Dom` (a `Vec`-indexed arena) and address nodes by `NodeId`, with node 0 (`DOCUMENT`) always the document. Every id stays valid for the life of the `Dom`, however the tree is mutated.

```rust
use fast5ever::{parse_fragment, DOCUMENT};

let mut dom = parse_fragment("<p>one<p>two", "body");
let p = dom.children(DOCUMENT)[0];
dom.set_attr(p, "class", "lead").unwrap();
let extra = parse_fragment("<b>!</b>", "body");
let extra = dom.import(&extra, DOCUMENT);   // cross-tree moves go through an explicit import
dom.append_child(p, extra).unwrap();        // a document node splices its children, as in Python
assert_eq!(dom.to_html(DOCUMENT), r#"<p class="lead">one<b>!</b></p><p>two</p>"#);
```

Reads mirror the Python names (`children`, `parent`, `attr`, `to_html`, `to_text`); writes return `Result` where Python raises (`set_attr`, `set_text`, `append_child`, `insert_before`, `replace_child`, `detach`); construction is `create_element`/`create_text`/`create_comment` (Python's `Element`/`Text`/`Comment`); and Rust additionally exposes `NodeData` matching for direct tree inspection.


## Development

```bash
pip install -e .[dev]
maturin develop && pytest -q
```

All tests are pytest; `cargo check`/`cargo clippy` stay warning-free and need no Python (pyo3 sits behind the `python` feature).

## Release

Release flow: tag-push (CI publishes), then the version bump, all in one command.

```bash
maturin develop && pytest -q
ship-release
```

The GitHub workflow builds wheels on tags matching `v*` and publishes them to GitHub Releases and PyPI.
