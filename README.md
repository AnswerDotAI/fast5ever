# fast5ever

`fast5ever` parses HTML into a mutable DOM for Python programs. You can inspect and edit nodes, construct elements, and serialize the result as HTML. It uses Servo's [html5ever](https://github.com/servo/html5ever) for WHATWG-compliant parsing and serialization.

html5ever implements the specification's algorithms and requires a separate tree implementation. fast5ever supplies an arena-based DOM and Python bindings. Parsing, error recovery, and serialization follow the same algorithms and produce the same results as a browser's `innerHTML`.

```python
from fast5ever import parse, parse_fragment

frag = parse_fragment('<p>one<p>two')
frag.to_html()                        # '<p>one</p><p>two</p>'
[c.name for c in frag.children]       # ['p', 'p']
frag.children[0].attrs['class'] = 'lead'   # attrs is live: writes go straight to the tree

from fast5ever import Span
frag.children[0].replace(Span('replacement', cls='lead'))

doc = parse('<!DOCTYPE html><title>t</title>hello')
doc.to_html()                         # '<!DOCTYPE html><html><head><title>t'...
```

## API

`parse(html)` parses a complete document. `parse_fragment(html, context='body')` parses a fragment in a context element. For example, use `context='tbody'` to parse table rows. Both functions return a `Document` node.

### Nodes and attributes

Every node is a `Document`, `Element`, `Text`, `Comment`, or `Doctype`. These classes inherit from `Node`. Use `isinstance(c, Text)` to check a node's type.

All nodes provide `.name`, `.children`, `.parent`, `to_html()`, and `to_text()`. An element's `.name` is its tag name. Other node names are `#document`, `#text`, `#comment`, and `#doctype`. Assigning `el.name = 'details'` renames an element in place and preserves its attributes and children.

For element queries:

- `.element_children` returns direct element children in order, including SVG/MathML elements but excluding text and comments. It does not enter template contents; use `template.content.element_children` for those.
- `el.is_tag('a')` matches an HTML anchor, not an SVG anchor. Pass a namespace URL explicitly for foreign elements: `el.is_tag('a', namespace='http://www.w3.org/2000/svg')`. Names are case-sensitive; `.name` remains the local name regardless of namespace.
- `el.has_class('lead')` checks a complete, case-sensitive class token, separated by HTML's ASCII whitespace. Non-elements return `False` for both predicates.

An undefined Python property reads the corresponding HTML attribute, with underscores converted to hyphens. For example, `el.data_op` reads `data-op`. An absent attribute raises `AttributeError`. Write attributes through `.attrs`.

`el.attrs` is a live mapping in source order. It supports the following operations:

- Read, set, and delete entries with `attrs['k']`, `attrs['k'] = v`, and `del attrs['k']`.
- Use `in`, `len`, and iteration as with a dictionary.
- Call `get`, `keys`, `values`, `items`, `update`, and `pop`.
- Compare with any mapping using `==`, or take a snapshot with `dict(attrs)`.

On non-element nodes, `.attrs` reads as an empty mapping and rejects writes.

`.text` contains a text or comment node's own content. Assign to it with `t.text = 'new'`. `template.content` returns a `<template>` element's contents as a `Document`.

### Constructing elements

`Element(name, attrs=None)`, `Text(text)`, and `Comment(text)` construct detached nodes for insertion.

Undefined capitalized module attributes create element factories using fastcore.xml naming conventions. For example, `from fast5ever import CustomTag` provides a factory for `<custom-tag>`. Calling `CustomTag('text', cls='x', data_kind='demo')` constructs `<custom-tag class="x" data-kind="demo">text</custom-tag>`.

Positional strings become escaped `Text` nodes. Positional nodes remain nodes. `fastcore.xml.Safe` and `fastcore.basics.NotStr` values contain trusted markup. The constructor parses these as fragments in the new element's context, including the context required for table children.

### Changing the tree

Use these methods to insert, replace, or remove nodes:

- `append_child(child)` appends a child.
- `insert_before(child, reference)` inserts a child before a reference node.
- `replace_child(new, old)` replaces a child.
- `old.replace(new)` is shorthand for `old.parent.replace_child(new, old)`.
- `el.unwrap()` replaces an element with its contents.
- `detach()` removes a node from its parent.

Inserting a `Document` inserts its children, following DocumentFragment semantics. For example, `old.replace(parse_fragment(markup))` replaces `old` with the parsed markup. Inserting a node from another tree deep-copies it. Node handles stay valid across all mutations.

The API uses WHATWG DOM terminology. Its Python node classes, live attribute mapping, `to_html()`, and `to_text()` are modeled on Emil Stenström's [JustHTML](https://github.com/EmilStenstrom/justhtml).

## Serialization and nesting

fast5ever uses html5ever's implementation of the WHATWG serialization algorithm. Its output matches Chrome's `innerHTML` byte for byte. The output conventions include:

- Boolean attributes have empty values, such as `open=""`.
- Attribute values use double quotes.
- Void elements have no closing `/`.
- Text inside `script` and `style` remains unescaped.

fast5ever provides no serialization formatting options or compatibility shims for other serializers.

Parsing flattens element nesting beyond 512 levels, matching Chromium's limit. This keeps parsing linear-time for deeply nested input. html5ever's tree builder alone takes quadratic time for that input.

## The Rust API

The Python API binds to the fast5ever Rust crate. Add the crate as a dependency with `fast5ever = { git = "https://github.com/AnswerDotAI/fast5ever" }`.

Both APIs provide the same tree operations. In Rust, you hold a `Dom` containing a `Vec`-indexed arena and address its nodes by `NodeId`. There are no separate node objects. Node 0 (`DOCUMENT`) is always the document. Every id stays valid for the life of the `Dom`, including after tree mutations.

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

Read methods use the Python names: `children`, `parent`, `attr`, `to_html`, and `to_text`. Mutation methods return `Result` where Python raises an exception. These include `set_attr`, `set_text`, `append_child`, `insert_before`, `replace_child`, and `detach`.

Element queries are `dom.element_children(id)`, `dom.tag(id)` (the local name, or `None` for non-elements), `dom.is_tag(id, "a", None)` (`None` selects HTML; `Some(url)` selects another namespace), and `dom.has_class(id, "lead")`.

Construct nodes with `create_element`, `create_text`, and `create_comment`, corresponding to Python's `Element`, `Text`, and `Comment`. Rust also provides `NodeData` matching for direct tree inspection.


## Development

```bash
pip install -e .[dev]
maturin develop && pytest -q
```

All tests use pytest. `cargo check` and `cargo clippy` run without warnings and do not need Python. The `python` feature enables pyo3.

## Release

`ship-release` pushes a tag for CI to publish, then bumps the version.

```bash
maturin develop && pytest -q
ship-release
```

The GitHub workflow builds wheels on tags matching `v*` and publishes them to GitHub Releases and PyPI.
