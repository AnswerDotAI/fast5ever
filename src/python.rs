//! Python bindings: `parse`, `parse_fragment`, and the `Node` handle.

use std::sync::{Arc, RwLock};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::dom::{Dom, DomError, NodeData, NodeId};

impl From<DomError> for PyErr {
    fn from(e: DomError) -> PyErr {
        PyValueError::new_err(e.to_string())
    }
}

/// A handle to one node of a parsed tree. Handles stay valid however the
/// tree is mutated.
#[pyclass(frozen, module = "fast5ever")]
pub struct Node {
    dom: Arc<RwLock<Dom>>,
    id: NodeId,
}

impl Node {
    fn wrap(&self, id: NodeId) -> Node {
        Node {
            dom: self.dom.clone(),
            id,
        }
    }

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

    /// Attributes as a dict in source order (a snapshot; empty for
    /// non-elements). Mutate via `set_attr`/`del_attr`.
    #[getter]
    fn attrs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        if let NodeData::Element { attrs, .. } = &self.dom.read().unwrap().get(self.id).data {
            for (n, v) in attrs {
                d.set_item(n.local.as_ref(), v)?;
            }
        }
        Ok(d)
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
    #[getter]
    fn text(&self) -> Option<String> {
        match &self.dom.read().unwrap().get(self.id).data {
            NodeData::Text { contents } | NodeData::Comment { contents } => Some(contents.clone()),
            _ => None,
        }
    }

    #[getter]
    fn children(&self) -> Vec<Node> {
        let dom = self.dom.read().unwrap();
        dom.children(self.id)
            .iter()
            .map(|&c| self.wrap(c))
            .collect()
    }

    #[getter]
    fn parent(&self) -> Option<Node> {
        self.dom
            .read()
            .unwrap()
            .parent(self.id)
            .map(|p| self.wrap(p))
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

    fn get_attr(&self, name: &str) -> Option<String> {
        self.dom
            .read()
            .unwrap()
            .attr(self.id, name)
            .map(str::to_string)
    }

    fn set_attr(&self, name: &str, value: &str) -> PyResult<()> {
        self.dom.write().unwrap().set_attr(self.id, name, value)?;
        Ok(())
    }

    fn del_attr(&self, name: &str) -> PyResult<Option<String>> {
        Ok(self.dom.write().unwrap().remove_attr(self.id, name)?)
    }

    /// Append `child` as the last child. A `#document` node splices its
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
        format!("<Node {}>", self.name())
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

/// Parse a complete HTML document; returns the `#document` node.
#[pyfunction]
fn parse(html: &str) -> Node {
    Node {
        dom: Arc::new(RwLock::new(crate::dom::parse(html))),
        id: crate::dom::DOCUMENT,
    }
}

/// Parse a fragment as the children of a `context` element (default `body`,
/// matching `innerHTML` on ordinary content); returns the `#document` node
/// holding the fragment's nodes.
#[pyfunction]
#[pyo3(signature = (html, context="body"))]
fn parse_fragment(html: &str, context: &str) -> Node {
    Node {
        dom: Arc::new(RwLock::new(crate::dom::parse_fragment(html, context))),
        id: crate::dom::DOCUMENT,
    }
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Node>()?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(parse_fragment, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
