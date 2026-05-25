//! Tree drawing renderer.

use crate::{RenderContext, RenderError};

/// Tree renderer for S7.6 terminal hierarchy output.
#[derive(Debug, Clone)]
pub struct TreeRenderer {
    ctx: RenderContext,
}

/// Tree node consumed by [`TreeRenderer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    /// Visible node label.
    pub label: String,
    /// Child nodes rendered below this node.
    pub children: Vec<Self>,
}

impl TreeRenderer {
    /// Builds a tree renderer with the supplied rendering context.
    #[must_use]
    pub const fn new(ctx: RenderContext) -> Self {
        Self { ctx }
    }

    /// Renders a tree rooted at `root`.
    ///
    /// # Errors
    ///
    /// This renderer currently has no fallible tree-specific path, but returns
    /// [`RenderError`] for API symmetry with other format renderers.
    pub fn render(&self, root: &TreeNode) -> Result<String, RenderError> {
        let chars = self.tree_chars();
        let mut lines = vec![root.label.clone()];

        push_children(root, "", &mut lines, chars);

        Ok(lines.join("\n"))
    }

    fn tree_chars(&self) -> TreeChars {
        if self.ctx.color && locale_supports_utf8(&self.ctx.locale) {
            TreeChars::utf8()
        } else {
            TreeChars::ascii()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TreeChars {
    branch: &'static str,
    last: &'static str,
    vertical: &'static str,
    empty: &'static str,
}

impl TreeChars {
    const fn utf8() -> Self {
        Self {
            branch: "├── ",
            last: "└── ",
            vertical: "│   ",
            empty: "    ",
        }
    }

    const fn ascii() -> Self {
        Self {
            branch: "|-- ",
            last: "`-- ",
            vertical: "|   ",
            empty: "    ",
        }
    }
}

fn push_children(node: &TreeNode, prefix: &str, lines: &mut Vec<String>, chars: TreeChars) {
    let child_count = node.children.len();

    for (index, child) in node.children.iter().enumerate() {
        let is_last = index + 1 == child_count;
        let connector = if is_last { chars.last } else { chars.branch };

        lines.push(format!("{prefix}{connector}{}", child.label));

        let next_prefix = if is_last {
            format!("{prefix}{}", chars.empty)
        } else {
            format!("{prefix}{}", chars.vertical)
        };

        push_children(child, &next_prefix, lines, chars);
    }
}

fn locale_supports_utf8(locale: &str) -> bool {
    let locale = locale.to_ascii_lowercase();
    locale.contains("utf-8") || locale.contains("utf8")
}
