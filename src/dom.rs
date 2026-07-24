//! Vec-indexed arena DOM: html5ever's `TreeSink` on the parse side, its
//! `Serialize` on the way back out, and a small mutation surface between.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::io;

use html5ever::buffer_queue::BufferQueue;
use html5ever::interface::{ElemName, ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::serialize::{Serialize, SerializeOpts, Serializer, TraversalScope};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{TokenSink, Tokenizer, TokenizerOpts};
use html5ever::tree_builder::{TreeBuilder, TreeBuilderOpts};
use html5ever::TokenizerResult;
use html5ever::{ns, Attribute, LocalName, Namespace, QualName};

use crate::depth::DepthCap;

pub type NodeId = usize;

/// One DOM node's payload. Elements hold their attributes in source order.
#[derive(Debug, Clone)]
pub enum NodeData {
    Document,
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
    Text {
        contents: String,
    },
    Comment {
        contents: String,
    },
    Element {
        name: QualName,
        attrs: Vec<(QualName, String)>,
        template_contents: Option<NodeId>,
    },
    ProcessingInstruction {
        target: String,
        contents: String,
    },
}

#[derive(Debug, Clone)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub data: NodeData,
}

/// A structural mutation that cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomError {
    NotAChild,
    WouldCycle,
    NotAnElement,
    NotTextual,
}

impl fmt::Display for DomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomError::NotAChild => write!(f, "reference node is not a child of this node"),
            DomError::WouldCycle => {
                write!(f, "inserting a node inside itself would create a cycle")
            }
            DomError::NotAnElement => write!(f, "operation requires an element node"),
            DomError::NotTextual => write!(f, "operation requires a text or comment node"),
        }
    }
}

impl std::error::Error for DomError {}

/// A parsed HTML tree. Node ids index into `nodes`; id 0 is the document
/// (for fragments, the fragment root). Ids stay valid for the life of the
/// `Dom`: detached nodes remain in the arena as orphans.
#[derive(Debug, Clone)]
pub struct Dom {
    pub nodes: Vec<Node>,
    pub quirks_mode: QuirksMode,
}

pub const DOCUMENT: NodeId = 0;

impl Dom {
    /// An empty tree: just a document node.
    pub fn new() -> Dom {
        Dom {
            nodes: vec![Node {
                parent: None,
                children: Vec::new(),
                data: NodeData::Document,
            }],
            quirks_mode: QuirksMode::NoQuirks,
        }
    }

    pub fn get(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.nodes[id].children
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes[id].parent
    }

    /// Element ids in document order from `id` (inclusive when `id` is an element).
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.serial_children(n).iter().rev());
        }
        out
    }

    /// The children serialization walks: a template element's contents stand in
    /// for its (always empty) structural children.
    fn serial_children(&self, id: NodeId) -> &[NodeId] {
        match &self.nodes[id].data {
            NodeData::Element {
                template_contents: Some(t),
                ..
            } => &self.nodes[*t].children,
            _ => &self.nodes[id].children,
        }
    }

    // --- creation ---

    fn push(&mut self, data: NodeData) -> NodeId {
        self.nodes.push(Node {
            parent: None,
            children: Vec::new(),
            data,
        });
        self.nodes.len() - 1
    }

    /// Create a detached html-namespace element.
    pub fn create_element(&mut self, name: &str, attrs: &[(&str, &str)]) -> NodeId {
        let qual = QualName::new(None, ns!(html), LocalName::from(name));
        let attrs = attrs
            .iter()
            .map(|(n, v)| {
                (
                    QualName::new(None, ns!(), LocalName::from(*n)),
                    v.to_string(),
                )
            })
            .collect();
        self.push(NodeData::Element {
            name: qual,
            attrs,
            template_contents: None,
        })
    }

    /// Create a detached text node.
    pub fn create_text(&mut self, text: &str) -> NodeId {
        self.push(NodeData::Text {
            contents: text.to_string(),
        })
    }

    /// Create a detached comment node.
    pub fn create_comment(&mut self, text: &str) -> NodeId {
        self.push(NodeData::Comment {
            contents: text.to_string(),
        })
    }

    /// Deep-copy `id` (and its subtree) from `other` into this arena, returning
    /// the copy's id, detached. The importing side of a cross-tree splice.
    pub fn import(&mut self, other: &Dom, id: NodeId) -> NodeId {
        let data = match &other.nodes[id].data {
            NodeData::Element {
                name,
                attrs,
                template_contents,
            } => NodeData::Element {
                name: name.clone(),
                attrs: attrs.clone(),
                template_contents: template_contents.map(|t| self.import(other, t)),
            },
            other_data => other_data.clone(),
        };
        let new_id = self.push(data);
        for child in other.nodes[id].children.clone() {
            let c = self.import(other, child);
            self.nodes[c].parent = Some(new_id);
            self.nodes[new_id].children.push(c);
        }
        new_id
    }

    // --- attributes ---

    pub fn attr(&self, id: NodeId, name: &str) -> Option<&str> {
        match &self.nodes[id].data {
            NodeData::Element { attrs, .. } => attrs
                .iter()
                .find(|(n, _)| &*n.local == name)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }

    /// Set (or add, preserving order for existing keys) an attribute.
    pub fn set_attr(&mut self, id: NodeId, name: &str, value: &str) -> Result<(), DomError> {
        match &mut self.nodes[id].data {
            NodeData::Element { attrs, .. } => {
                if let Some(slot) = attrs.iter_mut().find(|(n, _)| &*n.local == name) {
                    slot.1 = value.to_string();
                } else {
                    attrs.push((
                        QualName::new(None, ns!(), LocalName::from(name)),
                        value.to_string(),
                    ));
                }
                Ok(())
            }
            _ => Err(DomError::NotAnElement),
        }
    }

    /// Remove an attribute; returns its value if present.
    pub fn remove_attr(&mut self, id: NodeId, name: &str) -> Result<Option<String>, DomError> {
        match &mut self.nodes[id].data {
            NodeData::Element { attrs, .. } => {
                let pos = attrs.iter().position(|(n, _)| &*n.local == name);
                Ok(pos.map(|i| attrs.remove(i).1))
            }
            _ => Err(DomError::NotAnElement),
        }
    }

    // --- text ---

    /// Replace the contents of a text or comment node.
    pub fn set_text(&mut self, id: NodeId, text: &str) -> Result<(), DomError> {
        match &mut self.nodes[id].data {
            NodeData::Text { contents } | NodeData::Comment { contents } => {
                *contents = text.to_string();
                Ok(())
            }
            _ => Err(DomError::NotTextual),
        }
    }

    // --- structure ---

    /// Detach `id` from its parent (no-op when already detached).
    pub fn detach(&mut self, id: NodeId) {
        if let Some(p) = self.nodes[id].parent.take() {
            self.nodes[p].children.retain(|&c| c != id);
        }
    }

    fn is_ancestor(&self, maybe_ancestor: NodeId, mut node: NodeId) -> bool {
        loop {
            if node == maybe_ancestor {
                return true;
            }
            match self.nodes[node].parent {
                Some(p) => node = p,
                None => return false,
            }
        }
    }

    /// The ids an insertion of `child` will actually splice in: a document
    /// node inserts its children (fragment-splice semantics) and is left
    /// empty; anything else inserts itself.
    fn splice_ids(&mut self, child: NodeId) -> Vec<NodeId> {
        if matches!(self.nodes[child].data, NodeData::Document) {
            let kids = std::mem::take(&mut self.nodes[child].children);
            for &k in &kids {
                self.nodes[k].parent = None;
            }
            kids
        } else {
            self.detach(child);
            vec![child]
        }
    }

    fn insert_ids(&mut self, parent: NodeId, index: usize, ids: &[NodeId]) {
        for (off, &id) in ids.iter().enumerate() {
            self.nodes[parent].children.insert(index + off, id);
            self.nodes[id].parent = Some(parent);
        }
    }

    /// Append `child` as `parent`'s last child. A document node splices its
    /// children instead (and is left empty).
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), DomError> {
        self.insert_before(parent, child, None)
    }

    /// Insert `child` before `reference` (`None` appends). A document node
    /// splices its children instead (and is left empty).
    pub fn insert_before(
        &mut self,
        parent: NodeId,
        child: NodeId,
        reference: Option<NodeId>,
    ) -> Result<(), DomError> {
        if self.is_ancestor(child, parent) && !matches!(self.nodes[child].data, NodeData::Document)
        {
            return Err(DomError::WouldCycle);
        }
        let ids = self.splice_ids(child);
        let index = match reference {
            None => self.nodes[parent].children.len(),
            Some(r) => self.nodes[parent]
                .children
                .iter()
                .position(|&c| c == r)
                .ok_or(DomError::NotAChild)?,
        };
        self.insert_ids(parent, index, &ids);
        Ok(())
    }

    /// Replace `old` (a child of `parent`) with `new`; `old` is detached.
    pub fn replace_child(
        &mut self,
        parent: NodeId,
        new: NodeId,
        old: NodeId,
    ) -> Result<(), DomError> {
        if new == old {
            return Ok(());
        }
        self.insert_before(parent, new, Some(old))?;
        self.detach(old);
        Ok(())
    }

    /// Detach every child of `id`.
    pub fn clear_children(&mut self, id: NodeId) {
        for c in std::mem::take(&mut self.nodes[id].children) {
            self.nodes[c].parent = None;
        }
    }

    // --- output ---

    /// Serialize: elements include themselves, document nodes serialize their
    /// children (the fragment/document markup).
    pub fn to_html(&self, id: NodeId) -> String {
        let scope = match self.nodes[id].data {
            NodeData::Document => TraversalScope::ChildrenOnly(None),
            _ => TraversalScope::IncludeNode,
        };
        let mut buf = Vec::new();
        html5ever::serialize::serialize(
            &mut buf,
            &SerNode { dom: self, id },
            SerializeOpts {
                traversal_scope: scope,
                ..Default::default()
            },
        )
        .expect("serializing to a Vec cannot fail");
        String::from_utf8(buf).expect("serializer output is UTF-8")
    }

    /// Concatenated contents of all text descendants (raw text included).
    pub fn to_text(&self, id: NodeId) -> String {
        let mut out = String::new();
        for n in self.descendants(id) {
            if let NodeData::Text { contents } = &self.nodes[n].data {
                out.push_str(contents);
            }
        }
        out
    }
}

impl Default for Dom {
    fn default() -> Self {
        Dom::new()
    }
}

struct SerNode<'a> {
    dom: &'a Dom,
    id: NodeId,
}

impl Serialize for SerNode<'_> {
    fn serialize<S: Serializer>(
        &self,
        serializer: &mut S,
        traversal_scope: TraversalScope,
    ) -> io::Result<()> {
        let node = &self.dom.nodes[self.id];
        if let TraversalScope::ChildrenOnly(_) = traversal_scope {
            for &child in self.dom.serial_children(self.id) {
                SerNode {
                    dom: self.dom,
                    id: child,
                }
                .serialize(serializer, TraversalScope::IncludeNode)?;
            }
            return Ok(());
        }
        match &node.data {
            NodeData::Element { name, attrs, .. } => {
                serializer.start_elem(name.clone(), attrs.iter().map(|(n, v)| (n, v.as_str())))?;
                for &child in self.dom.serial_children(self.id) {
                    SerNode {
                        dom: self.dom,
                        id: child,
                    }
                    .serialize(serializer, TraversalScope::IncludeNode)?;
                }
                serializer.end_elem(name.clone())
            }
            NodeData::Text { contents } => serializer.write_text(contents),
            NodeData::Comment { contents } => serializer.write_comment(contents),
            NodeData::Doctype { name, .. } => serializer.write_doctype(name),
            NodeData::ProcessingInstruction { target, contents } => {
                serializer.write_processing_instruction(target, contents)
            }
            NodeData::Document => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "document nodes serialize children-only",
            )),
        }
    }
}

// --- parsing ---

struct Sink {
    nodes: RefCell<Vec<Node>>,
    quirks: Cell<QuirksMode>,
}

impl Sink {
    fn new() -> Sink {
        Sink {
            nodes: RefCell::new(vec![Node {
                parent: None,
                children: Vec::new(),
                data: NodeData::Document,
            }]),
            quirks: Cell::new(QuirksMode::NoQuirks),
        }
    }

    fn push(&self, data: NodeData) -> NodeId {
        let mut nodes = self.nodes.borrow_mut();
        nodes.push(Node {
            parent: None,
            children: Vec::new(),
            data,
        });
        nodes.len() - 1
    }

    fn append_node(&self, parent: NodeId, child: NodeId) {
        let mut nodes = self.nodes.borrow_mut();
        nodes[parent].children.push(child);
        nodes[child].parent = Some(parent);
    }

    fn append_text(&self, parent: NodeId, text: &str) {
        {
            let mut nodes = self.nodes.borrow_mut();
            if let Some(&last) = nodes[parent].children.last() {
                if let NodeData::Text { contents } = &mut nodes[last].data {
                    contents.push_str(text);
                    return;
                }
            }
        }
        let id = self.push(NodeData::Text {
            contents: text.to_string(),
        });
        self.append_node(parent, id);
    }

    fn insert_at(&self, parent: NodeId, index: usize, child: NodeId) {
        let mut nodes = self.nodes.borrow_mut();
        nodes[parent].children.insert(index, child);
        nodes[child].parent = Some(parent);
    }

    fn detach(&self, id: NodeId) {
        let mut nodes = self.nodes.borrow_mut();
        if let Some(p) = nodes[id].parent.take() {
            let pos = nodes[p].children.iter().position(|&c| c == id);
            if let Some(pos) = pos {
                nodes[p].children.remove(pos);
            }
        }
    }

    fn parent_and_index(&self, sibling: NodeId) -> (NodeId, usize) {
        let nodes = self.nodes.borrow();
        let parent = nodes[sibling].parent.expect("sibling has no parent");
        let index = nodes[parent]
            .children
            .iter()
            .position(|&c| c == sibling)
            .expect("sibling not among its parent's children");
        (parent, index)
    }
}

/// Owned element name, so `elem_name` needs no borrow into the arena.
#[derive(Debug)]
pub struct OwnedElemName {
    ns: Namespace,
    local: LocalName,
}

impl ElemName for OwnedElemName {
    fn ns(&self) -> &Namespace {
        &self.ns
    }

    fn local_name(&self) -> &LocalName {
        &self.local
    }
}

impl TreeSink for Sink {
    type Handle = NodeId;
    type Output = Dom;
    type ElemName<'a> = OwnedElemName;

    fn finish(self) -> Dom {
        Dom {
            nodes: self.nodes.into_inner(),
            quirks_mode: self.quirks.get(),
        }
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {}

    fn get_document(&self) -> NodeId {
        DOCUMENT
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> OwnedElemName {
        match &self.nodes.borrow()[*target].data {
            NodeData::Element { name, .. } => OwnedElemName {
                ns: name.ns.clone(),
                local: name.local.clone(),
            },
            _ => panic!("elem_name called on a non-element node"),
        }
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> NodeId {
        let template_contents = flags.template.then(|| self.push(NodeData::Document));
        self.push(NodeData::Element {
            name,
            attrs: attrs
                .into_iter()
                .map(|a| (a.name, a.value.to_string()))
                .collect(),
            template_contents,
        })
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.push(NodeData::Comment {
            contents: text.to_string(),
        })
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> NodeId {
        self.push(NodeData::ProcessingInstruction {
            target: target.to_string(),
            contents: data.to_string(),
        })
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        match child {
            NodeOrText::AppendNode(id) => self.append_node(*parent, id),
            NodeOrText::AppendText(text) => self.append_text(*parent, &text),
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        let has_parent = self.nodes.borrow()[*element].parent.is_some();
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let id = self.push(NodeData::Doctype {
            name: name.to_string(),
            public_id: public_id.to_string(),
            system_id: system_id.to_string(),
        });
        self.append_node(DOCUMENT, id);
    }

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        match self.nodes.borrow()[*target].data {
            NodeData::Element {
                template_contents: Some(t),
                ..
            } => t,
            _ => panic!("get_template_contents called on a non-template node"),
        }
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.quirks.set(mode);
    }

    fn append_before_sibling(&self, sibling: &NodeId, new_node: NodeOrText<NodeId>) {
        match new_node {
            NodeOrText::AppendText(text) => {
                let (parent, index) = self.parent_and_index(*sibling);
                {
                    let mut nodes = self.nodes.borrow_mut();
                    if index > 0 {
                        let prev = nodes[parent].children[index - 1];
                        if let NodeData::Text { contents } = &mut nodes[prev].data {
                            contents.push_str(&text);
                            return;
                        }
                    }
                }
                let id = self.push(NodeData::Text {
                    contents: text.to_string(),
                });
                self.insert_at(parent, index, id);
            }
            NodeOrText::AppendNode(id) => {
                self.detach(id);
                let (parent, index) = self.parent_and_index(*sibling);
                self.insert_at(parent, index, id);
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &NodeId, attrs: Vec<Attribute>) {
        let mut nodes = self.nodes.borrow_mut();
        if let NodeData::Element {
            attrs: existing, ..
        } = &mut nodes[*target].data
        {
            for attr in attrs {
                if !existing.iter().any(|(n, _)| *n == attr.name) {
                    existing.push((attr.name, attr.value.to_string()));
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        self.detach(*target);
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        let mut nodes = self.nodes.borrow_mut();
        let kids = std::mem::take(&mut nodes[*node].children);
        for &k in &kids {
            nodes[k].parent = Some(*new_parent);
        }
        nodes[*new_parent].children.extend(kids);
    }
}

/// Drive `html` through the tokenizer until done (scripts don't block us).
fn drive<S: TokenSink>(tok: &Tokenizer<S>, html: &str) {
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(html));
    while !matches!(tok.feed(&input), TokenizerResult::Done) {}
    tok.end();
}

/// Parse a complete HTML document. Nesting is capped Chromium-style at
/// [`MAX_DEPTH`](crate::MAX_DEPTH): deeper content parses flattened at the cap.
pub fn parse(html: &str) -> Dom {
    let tb = TreeBuilder::new(Sink::new(), TreeBuilderOpts::default());
    let tok = Tokenizer::new(DepthCap::new(tb), TokenizerOpts::default());
    drive(&tok, html);
    tok.sink.into_inner().sink.finish()
}

/// Parse an HTML fragment as the children of a `context` element (spec
/// fragment parsing; `"body"` matches what browsers do for `innerHTML` on
/// ordinary content). The result's document node holds the fragment's nodes
/// directly. Nesting is capped Chromium-style at [`MAX_DEPTH`](crate::MAX_DEPTH).
pub fn parse_fragment(html: &str, context: &str) -> Dom {
    let sink = Sink::new();
    let name = QualName::new(None, ns!(html), LocalName::from(context));
    let context_elem = html5ever::interface::create_element(&sink, name, Vec::new());
    let tb = TreeBuilder::new_for_fragment(sink, context_elem, None, TreeBuilderOpts::default());
    let tok_opts = TokenizerOpts {
        initial_state: Some(tb.tokenizer_state_for_context_elem(false)),
        ..Default::default()
    };
    let tok = Tokenizer::new(DepthCap::new(tb), tok_opts);
    drive(&tok, html);
    into_fragment(tok.sink.into_inner().sink.finish())
}

/// Fragment parses wrap their content in a synthetic `<html>` element: splice
/// that wrapper's children up into the document node.
fn into_fragment(mut dom: Dom) -> Dom {
    let wrapper = dom.nodes[DOCUMENT]
        .children
        .iter()
        .copied()
        .find(|&c| matches!(dom.nodes[c].data, NodeData::Element { .. }));
    if let Some(w) = wrapper {
        let kids = std::mem::take(&mut dom.nodes[w].children);
        for &k in &kids {
            dom.nodes[k].parent = Some(DOCUMENT);
        }
        let pos = dom.nodes[DOCUMENT]
            .children
            .iter()
            .position(|&c| c == w)
            .unwrap();
        dom.nodes[DOCUMENT].children.splice(pos..=pos, kids);
    }
    dom
}
