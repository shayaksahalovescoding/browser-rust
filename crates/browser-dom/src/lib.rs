//! Custom HTML5 parser, DOM tree, CSS3 cascade, block/inline/table/flex layout, and software painter.

pub mod css;
pub mod dom;
pub mod html;
pub mod layout;
pub mod paint;

pub use css::{ComputedStyle, Stylesheet, compute_styles};
pub use dom::{Document, Node, NodeData, NodeId};
pub use html::parse_html;
pub use layout::{ControlKind, HitTarget, LayoutBox, LayoutEngine};
pub use paint::{paint_layout, paint_layout_with_assets};
