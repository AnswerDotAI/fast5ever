//! Python bindings: `parse`/`parse_fragment`, the `Node` class hierarchy,
//! and the live `Attrs` mapping.

use std::sync::{Arc, RwLock};

use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PyTuple};

use crate::dom::{Dom, DomError, NodeData, NodeId};

impl From<DomError> for PyErr {
    fn from(e: DomError) -> PyErr {
        PyValueError::new_err(e.to_string())
    }
}

/// The base node handle: every node is an instance of one of the concrete
/// classes below (`isinstance(n, Node)` matches any kind). Handles stay
/// valid however the tree is mutated.
#[pyclass(frozen, subclass, module = "fast5ever")]
pub struct Node {
    dom: Arc<RwLock<Dom>>,
    id: NodeId,
}

/// An element. `Element(name, attrs=None)` creates a detached element to
/// insert into a tree.
#[pyclass(frozen, extends = Node, module = "fast5ever")]
pub struct Element;

/// A text node. `Text(text)` creates a detached one.
#[pyclass(frozen, extends = Node, module = "fast5ever")]
pub struct Text;

/// A comment node. `Comment(text)` creates a detached one.
#[pyclass(frozen, extends = Node, module = "fast5ever")]
pub struct Comment;

/// A doctype node.
#[pyclass(frozen, extends = Node, module = "fast5ever")]
pub struct Doctype;

/// The root of a parsed document or fragment. Inserting one into another
/// tree splices its children in (DocumentFragment semantics).
#[pyclass(frozen, extends = Node, module = "fast5ever")]
pub struct Document;

/// Wrap `id` in the concrete class matching its node kind.
fn make_node(py: Python<'_>, dom: Arc<RwLock<Dom>>, id: NodeId) -> PyResult<Py<PyAny>> {
    enum Kind {
        Doc,
        El,
        Text,
        Comment,
        Doctype,
        Other,
    }
    let kind = match &dom.read().unwrap().get(id).data {
        NodeData::Document => Kind::Doc,
        NodeData::Element { .. } => Kind::El,
        NodeData::Text { .. } => Kind::Text,
        NodeData::Comment { .. } => Kind::Comment,
        NodeData::Doctype { .. } => Kind::Doctype,
        NodeData::ProcessingInstruction { .. } => Kind::Other,
    };
    let init = PyClassInitializer::from(Node { dom, id });
    Ok(match kind {
        Kind::Doc => Py::new(py, init.add_subclass(Document))?.into_any(),
        Kind::El => Py::new(py, init.add_subclass(Element))?.into_any(),
        Kind::Text => Py::new(py, init.add_subclass(Text))?.into_any(),
        Kind::Comment => Py::new(py, init.add_subclass(Comment))?.into_any(),
        Kind::Doctype => Py::new(py, init.add_subclass(Doctype))?.into_any(),
        Kind::Other => Py::new(py, init)?.into_any(),
    })
}

/// A single-quoted, escaped, length-capped preview of text for reprs.
fn preview(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i == 30 {
            out.push('…');
            break;
        }
        out.extend(c.escape_debug());
    }
    format!("'{out}'")
}

impl Node {
    /// The other node's id in self's arena, importing a deep copy when the
    /// two handles belong to different trees.
    fn local_id(&self, other: &Node) -> NodeId {
        if Arc::ptr_eq(&self.dom, &other.dom) {
            other.id
        } else {
            let foreign = other.dom.read().unwrap();
            self.dom.write().unwrap().import(&foreign, other.id)
        }
    }
}

#[pymethods]
impl Node {
    /// Element tag name, or `#document`/`#text`/`#comment`/`#doctype`/`#pi`.
    #[getter]
    fn name(&self) -> String {
        match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Document => "#document".into(),
            NodeData::Doctype { .. } => "#doctype".into(),
            NodeData::Text { .. } => "#text".into(),
            NodeData::Comment { .. } => "#comment".into(),
            NodeData::ProcessingInstruction { .. } => "#pi".into(),
            NodeData::Element { name, .. } => name.local.to_string(),
        }
    }

    /// Rename the element in place (`el.name = "details"`), keeping its
    /// attributes and children; raises for non-elements.
    #[setter]
    fn set_name(&self, value: &str) -> PyResult<()> {
        Ok(self.dom.write().unwrap().rename(self.id, value)?)
    }

    /// The element's attributes as a live mapping: reads see the tree as it
    /// is, and `attrs[k] = v` / `del attrs[k]` write straight through.
    /// Empty and read-only for non-elements.
    #[getter]
    fn attrs(&self) -> Attrs {
        Attrs {
            dom: self.dom.clone(),
            id: self.id,
        }
    }

    /// Element namespace URL for non-HTML elements (SVG/MathML); `None` for
    /// HTML elements and non-elements.
    #[getter]
    fn namespace(&self) -> Option<String> {
        match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Element { name, .. } if *name.ns != *"http://www.w3.org/1999/xhtml" => {
                Some(name.ns.to_string())
            }
            _ => None,
        }
    }

    /// Own textual content: text/comment contents, `None` for other kinds.
    /// Settable on text and comment nodes.
    #[getter]
    fn text(&self) -> Option<String> {
        match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Text { contents } | NodeData::Comment { contents } => Some(contents.clone()),
            _ => None,
        }
    }

    #[setter]
    fn set_text(&self, value: &str) -> PyResult<()> {
        Ok(self.dom.write().unwrap().set_text(self.id, value)?)
    }

    #[getter]
    fn children(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let ids = self.dom.read().unwrap().children(self.id).to_vec();
        ids.into_iter()
            .map(|c| make_node(py, self.dom.clone(), c))
            .collect()
    }

    #[getter]
    fn parent(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let p = self.dom.read().unwrap().parent(self.id);
        p.map(|p| make_node(py, self.dom.clone(), p)).transpose()
    }

    /// A `<template>` element's contents as a `Document` node (they live
    /// outside its child list); `None` for anything else.
    #[getter]
    fn content(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let t = match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Element {
                template_contents: Some(t),
                ..
            } => Some(*t),
            _ => None,
        };
        t.map(|t| make_node(py, self.dom.clone(), t)).transpose()
    }

    /// Serialize this node (elements include themselves; a document node
    /// serializes its children).
    fn to_html(&self) -> String {
        self.dom.read().unwrap().to_html(self.id)
    }

    /// Concatenated text-node descendants.
    fn to_text(&self) -> String {
        self.dom.read().unwrap().to_text(self.id)
    }

    /// Append `child` as the last child. A `Document` node splices its
    /// children in (and is left empty); a node from another tree is
    /// deep-copied first.
    fn append_child(&self, child: &Node) -> PyResult<()> {
        let id = self.local_id(child);
        self.dom.write().unwrap().append_child(self.id, id)?;
        Ok(())
    }

    /// Insert `child` before `reference` (`None` appends), with the same
    /// splice/copy semantics as `append_child`.
    #[pyo3(signature = (child, reference=None))]
    fn insert_before(&self, child: &Node, reference: Option<&Node>) -> PyResult<()> {
        let id = self.local_id(child);
        let reference = reference.map(|r| r.id);
        self.dom
            .write()
            .unwrap()
            .insert_before(self.id, id, reference)?;
        Ok(())
    }

    /// Replace child `old` with `new` (same splice/copy semantics); `old` is
    /// detached but its handle stays usable.
    fn replace_child(&self, new: &Node, old: &Node) -> PyResult<()> {
        let id = self.local_id(new);
        self.dom
            .write()
            .unwrap()
            .replace_child(self.id, id, old.id)?;
        Ok(())
    }

    /// Detach this node from its parent (no-op when already detached).
    fn detach(&self) {
        self.dom.write().unwrap().detach(self.id);
    }

    fn __repr__(&self) -> String {
        match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Document => "<Document>".into(),
            NodeData::Element { name, .. } => format!("<Element {}>", name.local),
            NodeData::Text { contents } => format!("<Text {}>", preview(contents)),
            NodeData::Comment { contents } => format!("<Comment {}>", preview(contents)),
            NodeData::Doctype { name, .. } => format!("<Doctype {name}>"),
            NodeData::ProcessingInstruction { .. } => "<Node #pi>".into(),
        }
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.cast::<Node>() {
            Ok(o) => {
                let o: &Node = o.get();
                Arc::ptr_eq(&self.dom, &o.dom) && self.id == o.id
            }
            Err(_) => false,
        }
    }
}

/// A detached node in a fresh single-node arena, for the constructors.
fn detached(dom: Dom, id: NodeId) -> Node {
    Node {
        dom: Arc::new(RwLock::new(dom)),
        id,
    }
}

#[pymethods]
impl Element {
    #[new]
    #[pyo3(signature = (name, attrs=None))]
    fn new(name: &str, attrs: Option<&Bound<'_, PyDict>>) -> PyResult<PyClassInitializer<Element>> {
        let mut dom = Dom::new();
        let id = dom.create_element(name, &[]);
        if let Some(attrs) = attrs {
            for (k, v) in attrs {
                dom.set_attr(id, &k.extract::<String>()?, &v.extract::<String>()?)?;
            }
        }
        Ok(PyClassInitializer::from(detached(dom, id)).add_subclass(Element))
    }
}

#[pymethods]
impl Text {
    #[new]
    fn new(text: &str) -> PyClassInitializer<Text> {
        let mut dom = Dom::new();
        let id = dom.create_text(text);
        PyClassInitializer::from(detached(dom, id)).add_subclass(Text)
    }
}

#[pymethods]
impl Comment {
    #[new]
    fn new(text: &str) -> PyClassInitializer<Comment> {
        let mut dom = Dom::new();
        let id = dom.create_comment(text);
        PyClassInitializer::from(detached(dom, id)).add_subclass(Comment)
    }
}

/// One element's attributes as a live mapping: reads always see the tree as
/// it is, and writes go straight to it. Compares equal to any mapping with
/// the same items; `dict(attrs)` takes a snapshot.
#[pyclass(frozen, module = "fast5ever")]
pub struct Attrs {
    dom: Arc<RwLock<Dom>>,
    id: NodeId,
}

impl Attrs {
    fn snapshot<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        if let NodeData::Element { attrs, .. } = &self.dom.read().unwrap().get(self.id).data {
            for (n, v) in attrs {
                d.set_item(n.local.as_ref(), v)?;
            }
        }
        Ok(d)
    }
}

#[pymethods]
impl Attrs {
    fn __getitem__(&self, key: &str) -> PyResult<String> {
        self.dom
            .read()
            .unwrap()
            .attr(self.id, key)
            .map(str::to_string)
            .ok_or_else(|| PyKeyError::new_err(key.to_string()))
    }

    fn __setitem__(&self, key: &str, value: &str) -> PyResult<()> {
        Ok(self.dom.write().unwrap().set_attr(self.id, key, value)?)
    }

    fn __delitem__(&self, key: &str) -> PyResult<()> {
        match self.dom.write().unwrap().remove_attr(self.id, key)? {
            Some(_) => Ok(()),
            None => Err(PyKeyError::new_err(key.to_string())),
        }
    }

    fn __contains__(&self, key: &str) -> bool {
        self.dom.read().unwrap().attr(self.id, key).is_some()
    }

    fn __len__(&self) -> usize {
        match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Element { attrs, .. } => attrs.len(),
            _ => 0,
        }
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        Ok(PyList::new(py, self.keys())?.as_any().try_iter()?.unbind())
    }

    fn keys(&self) -> Vec<String> {
        match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Element { attrs, .. } => {
                attrs.iter().map(|(n, _)| n.local.to_string()).collect()
            }
            _ => Vec::new(),
        }
    }

    fn values(&self) -> Vec<String> {
        match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Element { attrs, .. } => attrs.iter().map(|(_, v)| v.clone()).collect(),
            _ => Vec::new(),
        }
    }

    fn items(&self) -> Vec<(String, String)> {
        match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Element { attrs, .. } => attrs
                .iter()
                .map(|(n, v)| (n.local.to_string(), v.clone()))
                .collect(),
            _ => Vec::new(),
        }
    }

    #[pyo3(signature = (key, default=None))]
    fn get(&self, py: Python<'_>, key: &str, default: Option<Py<PyAny>>) -> PyResult<Py<PyAny>> {
        match self.dom.read().unwrap().attr(self.id, key) {
            Some(v) => Ok(v.into_pyobject(py)?.into_any().unbind()),
            None => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    #[pyo3(signature = (key, *default))]
    fn pop(&self, py: Python<'_>, key: &str, default: &Bound<'_, PyTuple>) -> PyResult<Py<PyAny>> {
        match self.dom.write().unwrap().remove_attr(self.id, key)? {
            Some(v) => Ok(v.into_pyobject(py)?.into_any().unbind()),
            None if default.is_empty() => Err(PyKeyError::new_err(key.to_string())),
            None => Ok(default.get_item(0)?.unbind()),
        }
    }

    fn update(&self, other: &Bound<'_, PyAny>) -> PyResult<()> {
        for pair in other.call_method0("items")?.try_iter()? {
            let (k, v): (String, String) = pair?.extract()?;
            self.dom.write().unwrap().set_attr(self.id, &k, &v)?;
        }
        Ok(())
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let d = self.snapshot(other.py())?;
        match other.cast::<Attrs>() {
            Ok(o) => d.eq(o.get().snapshot(other.py())?),
            Err(_) => d.eq(other),
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(self.snapshot(py)?.to_string())
    }
}

/// Parse a complete HTML document; returns the `Document` node.
#[pyfunction]
fn parse(py: Python<'_>, html: &str) -> PyResult<Py<Document>> {
    let dom = Arc::new(RwLock::new(crate::dom::parse(html)));
    Py::new(
        py,
        PyClassInitializer::from(Node {
            dom,
            id: crate::dom::DOCUMENT,
        })
        .add_subclass(Document),
    )
}

/// Parse a fragment as the children of a `context` element (default `body`,
/// matching `innerHTML` on ordinary content); returns the `Document` node
/// holding the fragment's nodes.
#[pyfunction]
#[pyo3(signature = (html, context="body"))]
fn parse_fragment(py: Python<'_>, html: &str, context: &str) -> PyResult<Py<Document>> {
    let dom = Arc::new(RwLock::new(crate::dom::parse_fragment(html, context)));
    Py::new(
        py,
        PyClassInitializer::from(Node {
            dom,
            id: crate::dom::DOCUMENT,
        })
        .add_subclass(Document),
    )
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Node>()?;
    m.add_class::<Document>()?;
    m.add_class::<Element>()?;
    m.add_class::<Text>()?;
    m.add_class::<Comment>()?;
    m.add_class::<Doctype>()?;
    m.add_class::<Attrs>()?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(parse_fragment, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
