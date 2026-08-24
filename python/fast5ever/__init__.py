from fastcore.basics import NotStr
from fastcore.xml import attrmap, mk_getattr

from ._core import Attrs, Comment, Doctype, Document, Element, Node, Text, __version__, parse, parse_fragment

__all__ = ["__version__", "parse", "parse_fragment", "Node", "Document", "Element", "Text", "Comment", "Doctype", "Attrs"]


def _element(tag, *children, **attrs):
    el = Element(tag, {attrmap(k): v for k, v in attrs.items()})
    for child in children:
        if hasattr(child, "__html__"): child = parse_fragment(str(child.__html__()), context=tag)
        elif isinstance(child, NotStr): child = parse_fragment(str(child), context=tag)
        elif isinstance(child, str): child = Text(child)
        if not isinstance(child, Node): raise TypeError("element children must be str, Safe, NotStr, or Node")
        el.append_child(child)
    return el
__getattr__ = mk_getattr(_element)
