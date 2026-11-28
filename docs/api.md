---
title: API
description: Parse, inspect, mutate, and serialize HTML5 trees in Python
module: fast5ever
---

`fast5ever` parses HTML into a mutable, arena-backed DOM backed by Servo's `html5ever`. Parsing, error recovery, and serialization follow the WHATWG HTML specification.

## Parsing

Use `parse` for full HTML documents and `parse_fragment` for snippets parsed inside a specific HTML context element (such as `context='tbody'` for table rows). Both return a `Document` node.

```python
frag = parse_fragment('<p>one<p>two')
frag.to_html()                   # '<p>one</p><p>two</p>'
[c.name for c in frag.children]  # ['p', 'p']
```

{{< doc parse >}}

{{< doc parse_fragment >}}

## Nodes and attributes

Every node is an instance of `Document`, `Element`, `Text`, `Comment`, or `Doctype`, all inheriting from `Node`. Node handles stay valid across tree mutations.

All nodes expose `.name`, `.children`, `.element_children`, `.parent`, `.attrs`, `.text`, `.namespace`, and `.content` (the `Document` holding a `<template>` element's inert children). Assigning `el.name = 'section'` renames an element in place while keeping its attributes and children. Assigning `t.text = 'updated'` replaces text or comment content.

Reading an unknown attribute on an `Element` maps underscores to hyphens and reads the corresponding HTML attribute (`el.data_op` reads `data-op`), raising `AttributeError` when absent. Mutate attributes through the live `.attrs` mapping.

{{< doc Node >}}

{{< doc Document >}}

{{< doc Element >}}

{{< doc Text >}}

{{< doc Comment >}}

{{< doc Doctype >}}

{{< doc Attrs >}}

You can also import capitalized tag factories directly from `fast5ever` using `fastcore.xml` attribute naming (`cls` becomes `class`, underscores become hyphens). Positional strings become escaped `Text` nodes, while `fastcore.xml.Safe` and `fastcore.basics.NotStr` values are parsed as HTML fragments in the new element's context.

```python
from fast5ever import Div, Span

box = Div(Span('hello', cls='lead'), data_kind='demo')
box.to_html()  # '<div data-kind="demo"><span class="lead">hello</span></div>'
```

## Querying and mutating

`is_tag` matches an exact local tag name and namespace (`None` for HTML, or a namespace URL for SVG and MathML). `has_class` checks a single case-sensitive class token.

{{< doc Node.is_tag >}}

{{< doc Node.has_class >}}

Inserting a `Document` node splices its children into the target tree and leaves the `Document` empty. Inserting a node from a different tree deep-copies it first.

{{< doc Node.append_child >}}

{{< doc Node.insert_before >}}

{{< doc Node.replace_child >}}

{{< doc Node.replace >}}

{{< doc Node.unwrap >}}

{{< doc Node.detach >}}

## Serialization

{{< doc Node.to_html >}}

{{< doc Node.to_text >}}
