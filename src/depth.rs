//! Blink-style nesting cap. Browsers bound DOM depth (Chromium flattens at
//! 512); html5ever does not, and its tree builder's open-elements scans make
//! adversarially deep input quadratic. This token filter sits between the
//! tokenizer and the tree builder, swallowing start tags beyond [`MAX_DEPTH`]
//! (and their matching end tags), so deep content parses flattened at the cap
//! in linear time, roughly as Chromium renders it.
//!
//! Depth tracking is an approximation of the tree builder's stack (like
//! Chromium's own construction-site cap): void and foreign self-closing tags
//! do not deepen, and elements that implicitly close a same-named sibling
//! (`p`, `li`, `td`, ...) do not count sibling runs toward depth. Beyond the
//! cap, swallowed raw-text elements (e.g. a 513-deep `script`) lose their
//! raw-text tokenizer mode; their content still parses, just as markup.

use std::cell::RefCell;

use html5ever::tokenizer::{TagKind, Token, TokenSink, TokenSinkResult};
use html5ever::LocalName;

/// Maximum element nesting depth, matching Chromium.
pub const MAX_DEPTH: usize = 512;

/// HTML void elements: their start tags never deepen the tree.
fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "basefont"
            | "bgsound"
            | "br"
            | "col"
            | "embed"
            | "frame"
            | "hr"
            | "img"
            | "input"
            | "keygen"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Elements whose start tag implicitly closes an open same-named element, so
/// sibling runs of sloppy markup (`<p><p><p>...`) don't count toward depth.
fn closes_same(name: &str) -> bool {
    matches!(
        name,
        "p" | "li"
            | "dt"
            | "dd"
            | "tr"
            | "td"
            | "th"
            | "option"
            | "optgroup"
            | "caption"
            | "colgroup"
            | "thead"
            | "tbody"
            | "tfoot"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
    )
}

pub struct DepthCap<S> {
    inner: S,
    open: RefCell<Vec<LocalName>>,
    swallowed: RefCell<Vec<LocalName>>,
}

impl<S> DepthCap<S> {
    pub fn new(inner: S) -> DepthCap<S> {
        DepthCap {
            inner,
            open: RefCell::new(Vec::new()),
            swallowed: RefCell::new(Vec::new()),
        }
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: TokenSink> TokenSink for DepthCap<S> {
    type Handle = S::Handle;

    fn process_token(&self, token: Token, line_number: u64) -> TokenSinkResult<S::Handle> {
        if let Token::TagToken(tag) = &token {
            match tag.kind {
                TagKind::StartTag => {
                    // The html tree construction algorithm ignores the
                    // self-closing slash except in foreign (SVG/MathML)
                    // content, so only skip counting there or for voids.
                    let foreign_self_closing = tag.self_closing
                        && self
                            .inner
                            .adjusted_current_node_present_but_not_in_html_namespace();
                    if !is_void(&tag.name) && !foreign_self_closing {
                        let mut open = self.open.borrow_mut();
                        if closes_same(&tag.name) && open.last() == Some(&tag.name) {
                            // replaces its sibling: depth unchanged
                        } else if open.len() >= MAX_DEPTH {
                            self.swallowed.borrow_mut().push(tag.name.clone());
                            return TokenSinkResult::Continue;
                        } else {
                            open.push(tag.name.clone());
                        }
                    }
                }
                TagKind::EndTag => {
                    let mut swallowed = self.swallowed.borrow_mut();
                    if swallowed.last() == Some(&tag.name) {
                        swallowed.pop();
                        return TokenSinkResult::Continue;
                    }
                    let mut open = self.open.borrow_mut();
                    if let Some(pos) = open.iter().rposition(|n| *n == tag.name) {
                        open.truncate(pos);
                    }
                }
            }
        }
        self.inner.process_token(token, line_number)
    }

    fn end(&self) {
        self.inner.end();
    }

    fn adjusted_current_node_present_but_not_in_html_namespace(&self) -> bool {
        self.inner
            .adjusted_current_node_present_but_not_in_html_namespace()
    }
}
