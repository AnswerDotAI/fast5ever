"""fast5ever behavior tests. Expected strings are the WHATWG serialization
algorithm's spelling (what a browser's innerHTML produces), since fast5ever
serializes with html5ever's own serializer."""
import pytest
from fast5ever import Comment, Doctype, Document, Element, Node, Text, parse, parse_fragment


def test_document_roundtrip():
    src = '<!DOCTYPE html><html><head><title>t</title></head><body><p>hi</p></body></html>'
    assert parse(src).to_html() == src


def test_document_completion(): assert parse('<p>hi').to_html() == '<html><head></head><body><p>hi</p></body></html>'


def test_fragment():
    frag = parse_fragment('<p>one</p><p>two</p>')
    assert frag.to_html() == '<p>one</p><p>two</p>'
    assert frag.name == '#document'
    assert [c.name for c in frag.children] == ['p', 'p']


def test_tag_soup_correction():
    assert parse_fragment('<p>one<p>two').to_html() == '<p>one</p><p>two</p>'
    assert parse_fragment('<b>x<i>y</b>z</i>').to_html() == '<b>x<i>y</i></b><i>z</i>'


def test_fragment_context():
    assert parse_fragment('<tr><td>x</td></tr>', context='tbody').to_html() == '<tr><td>x</td></tr>'
    assert parse_fragment('<tr><td>x</td></tr>').to_html() == 'x'  # body context drops stray table structure


def test_table_gets_tbody():
    src = '<table><tr><td>x</td></tr></table>'
    assert parse_fragment(src).to_html() == '<table><tbody><tr><td>x</td></tr></tbody></table>'


def test_escaping():
    frag = parse_fragment('<p title="a&amp;b">x &lt; y &amp; z</p>')
    assert frag.to_html() == '<p title="a&amp;b">x &lt; y &amp; z</p>'
    assert frag.to_text() == 'x < y & z'


def test_spec_spellings():
    # boolean attributes keep their (empty) values, as innerHTML spells them
    assert parse_fragment('<details open></details>').to_html() == '<details open=""></details>'
    # attribute values always double-quoted
    assert parse_fragment("<p title='x'>t</p>").to_html() == '<p title="x">t</p>'
    # voids have no end tag or self-closing slash
    assert parse_fragment('<br/>').to_html() == '<br>'


def test_raw_text_elements():
    src = '<script>if (a < b && c) x()</script>'
    frag = parse_fragment(src)
    assert frag.to_html() == src
    assert frag.children[0].children[0].text == 'if (a < b && c) x()'


def test_comment():
    frag = parse_fragment('x<!-- c -->')
    assert frag.to_html() == 'x<!-- c -->'
    comment = frag.children[1]
    assert comment.name == '#comment' and comment.text == ' c '


def test_navigation():
    frag = parse_fragment('<div id="d" class="c"><span>s</span>tail</div>')
    div = frag.children[0]
    assert div.name == 'div'
    assert div.attrs == {'id': 'd', 'class': 'c'}
    assert div.attrs.get('id') == 'd' and div.attrs.get('missing') is None
    span, tail = div.children
    assert span.name == 'span'
    assert tail.name == '#text' and tail.text == 'tail' and tail.attrs == {}
    assert tail.parent == div and div.parent == frag
    assert frag.parent is None
    assert div != span and div == frag.children[0] and div != 'div'


def test_to_text(): assert parse_fragment('<p>a<b>b</b></p><p>c</p>').to_text() == 'abc'


def test_repr():
    frag = parse_fragment('<em>x</em>tail')
    assert repr(frag) == '<Document>'
    assert repr(frag.children[0]) == '<Element em>'
    assert repr(frag.children[1]) == "<Text 'tail'>"


def test_attr_mutation():
    frag = parse_fragment('<p id="a">x</p>')
    a = frag.children[0].attrs
    a['id'] = 'b'
    a['class'] = 'c'
    assert frag.to_html() == '<p id="b" class="c">x</p>'  # writes go straight to the tree
    del a['id']
    assert frag.to_html() == '<p class="c">x</p>'
    assert 'class' in a and 'id' not in a
    assert a['class'] == 'c' and a.get('id') is None and a.get('id', 'z') == 'z'
    with pytest.raises(KeyError): a['id']
    with pytest.raises(KeyError): del a['id']


def test_structure_mutation():
    frag = parse_fragment('<ul><li>a</li><li>c</li></ul>')
    ul = frag.children[0]
    li_b = parse_fragment('<li>b</li>', context='ul').children[0]
    ul.insert_before(li_b, ul.children[1])
    assert frag.to_html() == '<ul><li>a</li><li>b</li><li>c</li></ul>'
    ul.children[0].detach()
    assert frag.to_html() == '<ul><li>b</li><li>c</li></ul>'
    repl = parse_fragment('<li>z</li>', context='ul').children[0]
    ul.replace_child(repl, ul.children[1])
    assert frag.to_html() == '<ul><li>b</li><li>z</li></ul>'


def test_fragment_splice():
    # appending a #document node splices its children, like a DocumentFragment
    frag = parse_fragment('<div>start</div>')
    div = frag.children[0]
    div.append_child(parse_fragment('<b>1</b>2'))
    assert frag.to_html() == '<div>start<b>1</b>2</div>'


def test_replace_with_fragment():
    frag = parse_fragment('<div><span>old</span></div>')
    div = frag.children[0]
    div.replace_child(parse_fragment('a<b>c</b>'), div.children[0])
    assert frag.to_html() == '<div>a<b>c</b></div>'


def test_cross_tree_copy():
    a = parse_fragment('<p>text</p>')
    b = parse_fragment('<div></div>')
    b.children[0].append_child(a.children[0])
    assert b.to_html() == '<div><p>text</p></div>'
    assert a.to_html() == '<p>text</p>'  # source tree untouched: cross-tree moves copy


def test_cycle_rejected():
    frag = parse_fragment('<div><span>s</span></div>')
    div = frag.children[0]
    with pytest.raises(ValueError): div.children[0].append_child(div)


def test_template():
    src = '<template><p>t</p></template>'
    assert parse_fragment(src).to_html() == src


def test_version():
    import fast5ever
    assert fast5ever.__version__


def test_depth_cap():
    # Chromium-style nesting cap: element depth beyond 512 flattens at the cap
    deep = '<div>' * 600 + 'x' + '</div>' * 600
    out = parse_fragment(deep).to_html()
    assert out.count('<div>') == 512 and 'x' in out
    # sibling runs of implicitly-self-closing elements are not depth
    flat = parse_fragment('<p>a' * 600)
    assert flat.to_html().count('<p>') == 600


def test_node_classes():
    frag = parse_fragment('<p id="x">hi<!-- c --></p>')
    p = frag.children[0]
    text, comment = p.children
    assert isinstance(frag, Document) and isinstance(p, Element)
    assert isinstance(text, Text) and isinstance(comment, Comment)
    assert all(isinstance(n, Node) for n in (frag, p, text, comment))
    assert not isinstance(text, Element) and not isinstance(p, Text)
    doc = parse('<!DOCTYPE html><p>x')
    assert isinstance(doc.children[0], Doctype)


def test_node_construction():
    div = Element('div', {'id': 'd'})
    assert isinstance(div, Element) and div.parent is None
    div.append_child(Text('hi '))
    b = Element('b')
    b.append_child(Text('there'))
    div.append_child(b)
    div.append_child(Comment('note'))
    assert div.to_html() == '<div id="d">hi <b>there</b><!--note--></div>'


def test_text_mutation():
    frag = parse_fragment('<p>old</p><!-- c -->')
    frag.children[0].children[0].text = 'new'
    frag.children[1].text = ' d '
    assert frag.to_html() == '<p>new</p><!-- d -->'
    with pytest.raises(ValueError): frag.children[0].text = 'x'  # an element has no settable text


def test_template_content():
    frag = parse_fragment('<template><p>t</p></template><p>n</p>')
    tpl = frag.children[0]
    assert tpl.children == []  # contents live outside the child list
    content = tpl.content
    assert isinstance(content, Document) and content.to_html() == '<p>t</p>'
    content.children[0].attrs['id'] = 'i'
    assert frag.to_html() == '<template><p id="i">t</p></template><p>n</p>'
    assert frag.children[1].content is None


def test_attrs_iteration_and_update():
    a = parse_fragment('<p id="a" class="c">x</p>').children[0].attrs
    assert list(a) == ['id', 'class'] and a.keys() == ['id', 'class']
    assert a.values() == ['a', 'c'] and a.items() == [('id', 'a'), ('class', 'c')]
    assert dict(a) == {'id': 'a', 'class': 'c'} and repr(a) == "{'id': 'a', 'class': 'c'}"
    a.update({'class': 'k', 'data-x': 'y'})
    assert a == {'id': 'a', 'class': 'k', 'data-x': 'y'}
    assert a.pop('data-x') == 'y' and a.pop('data-x', None) is None
    with pytest.raises(KeyError): a.pop('data-x')


def test_attrs_non_element():
    frag = parse_fragment('x')
    t = frag.children[0]
    assert t.attrs == {} and len(t.attrs) == 0 and t.attrs.get('k') is None
    for node in (t, frag):
        with pytest.raises(ValueError): node.attrs['k'] = 'v'
