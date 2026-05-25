//! Fixed-width CLI table renderer.

use crate::{RenderContext, RenderError};

/// Table renderer for S7.6 terminal tables.
#[derive(Debug, Clone)]
pub struct TableRenderer {
    ctx: RenderContext,
}

/// Declarative table input used by cross-crate [`Renderable`](crate::Renderable)
/// implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSpec {
    /// Header labels, one per column.
    pub headers: Vec<String>,
    /// Body rows. Missing cells are rendered as empty strings.
    pub rows: Vec<Vec<String>>,
    /// Optional alignment per column. Missing entries default to left aligned.
    pub align: Vec<TableAlign>,
}

/// Horizontal alignment for table cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlign {
    /// Pad on the right.
    Left,
    /// Pad on the left.
    Right,
    /// Split padding across both sides.
    Center,
}

impl TableRenderer {
    /// Builds a table renderer with the supplied rendering context.
    #[must_use]
    pub const fn new(ctx: RenderContext) -> Self {
        Self { ctx }
    }

    /// Renders a fixed-width table.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::WidthOverflow`] when the computed table width
    /// exceeds [`RenderContext::width`].
    pub fn render(&self, spec: &TableSpec) -> Result<String, RenderError> {
        let column_count = column_count(spec);

        if column_count == 0 {
            return Ok(String::new());
        }

        let widths = column_widths(spec, column_count);
        let needed = rendered_width(&widths)?;

        if let Some(available) = self.ctx.width {
            if needed > u32::from(available) {
                return Err(RenderError::WidthOverflow { needed, available });
            }
        }

        let chars = self.table_chars();
        let mut lines = Vec::with_capacity(spec.rows.len() + 4);

        lines.push(border_line(
            chars.top_left,
            chars.top_separator,
            chars.top_right,
            chars.horizontal,
            &widths,
        ));
        lines.push(row_line(
            chars.vertical,
            &spec.headers,
            &widths,
            &[TableAlign::Left],
        ));
        lines.push(border_line(
            chars.middle_left,
            chars.middle_separator,
            chars.middle_right,
            chars.horizontal,
            &widths,
        ));

        for row in &spec.rows {
            lines.push(row_line(chars.vertical, row, &widths, &spec.align));
        }

        lines.push(border_line(
            chars.bottom_left,
            chars.bottom_separator,
            chars.bottom_right,
            chars.horizontal,
            &widths,
        ));

        Ok(lines.join("\n"))
    }

    fn table_chars(&self) -> TableChars {
        if self.ctx.color && locale_supports_utf8(&self.ctx.locale) {
            TableChars::utf8()
        } else {
            TableChars::ascii()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TableChars {
    top_left: &'static str,
    top_separator: &'static str,
    top_right: &'static str,
    middle_left: &'static str,
    middle_separator: &'static str,
    middle_right: &'static str,
    bottom_left: &'static str,
    bottom_separator: &'static str,
    bottom_right: &'static str,
    vertical: &'static str,
    horizontal: char,
}

impl TableChars {
    const fn utf8() -> Self {
        Self {
            top_left: "┌",
            top_separator: "┬",
            top_right: "┐",
            middle_left: "├",
            middle_separator: "┼",
            middle_right: "┤",
            bottom_left: "└",
            bottom_separator: "┴",
            bottom_right: "┘",
            vertical: "│",
            horizontal: '─',
        }
    }

    const fn ascii() -> Self {
        Self {
            top_left: "+",
            top_separator: "+",
            top_right: "+",
            middle_left: "+",
            middle_separator: "+",
            middle_right: "+",
            bottom_left: "+",
            bottom_separator: "+",
            bottom_right: "+",
            vertical: "|",
            horizontal: '-',
        }
    }
}

fn column_count(spec: &TableSpec) -> usize {
    spec.rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(spec.headers.len())
}

fn column_widths(spec: &TableSpec, column_count: usize) -> Vec<usize> {
    (0..column_count)
        .map(|column| {
            let header_width = spec
                .headers
                .get(column)
                .map_or(0, |header| display_width(header));
            let row_width = spec
                .rows
                .iter()
                .filter_map(|row| row.get(column))
                .map(|cell| display_width(cell))
                .max()
                .unwrap_or(0);

            header_width.max(row_width)
        })
        .collect()
}

fn rendered_width(widths: &[usize]) -> Result<u32, RenderError> {
    let content_width = widths.iter().sum::<usize>();
    let padding_width = widths.len() * 2;
    let border_width = widths.len() + 1;
    let needed = content_width + padding_width + border_width;

    u32::try_from(needed)
        .map_err(|_| RenderError::Internal("rendered table width does not fit in u32".to_owned()))
}

fn border_line(
    left: &str,
    separator: &str,
    right: &str,
    horizontal: char,
    widths: &[usize],
) -> String {
    let spans = widths
        .iter()
        .map(|width| horizontal.to_string().repeat(width + 2))
        .collect::<Vec<_>>();

    format!("{left}{}{right}", spans.join(separator))
}

fn row_line(vertical: &str, cells: &[String], widths: &[usize], align: &[TableAlign]) -> String {
    let mut line = String::new();
    line.push_str(vertical);

    for (column, width) in widths.iter().enumerate() {
        let cell = cells.get(column).map_or("", String::as_str);
        let align = align.get(column).copied().unwrap_or(TableAlign::Left);
        let padded = align_cell(cell, *width, align);

        line.push(' ');
        line.push_str(&padded);
        line.push(' ');
        line.push_str(vertical);
    }

    line
}

fn align_cell(value: &str, width: usize, align: TableAlign) -> String {
    let padding = width.saturating_sub(display_width(value));

    match align {
        TableAlign::Left => format!("{value}{}", " ".repeat(padding)),
        TableAlign::Right => format!("{}{value}", " ".repeat(padding)),
        TableAlign::Center => {
            let left = padding / 2;
            let right = padding - left;
            format!("{}{value}{}", " ".repeat(left), " ".repeat(right))
        }
    }
}

fn display_width(value: &str) -> usize {
    value.chars().count()
}

fn locale_supports_utf8(locale: &str) -> bool {
    let locale = locale.to_ascii_lowercase();
    locale.contains("utf-8") || locale.contains("utf8")
}
