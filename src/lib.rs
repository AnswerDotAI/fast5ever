//! fast5ever: WHATWG-compliant HTML parsing, mutation, and serialization,
//! powered by html5ever. The output spelling is the engine's serializer,
//! exactly as the WHATWG algorithm (and a browser's `innerHTML`) spells it.

mod depth;
mod dom;
#[cfg(feature = "python")]
mod python;

pub use depth::MAX_DEPTH;
pub use dom::{parse, parse_fragment, Dom, DomError, Node, NodeData, NodeId, DOCUMENT};
