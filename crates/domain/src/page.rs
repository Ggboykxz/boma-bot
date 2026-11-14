//! Bot-built pages (REQ-037, REQ-039): the block document, its limits, and the share token.
//!
//! A page is a static document of blocks. There is no script, no iframe, no foreign embed in the
//! blocks themselves: the only interactivity comes from the page runtime, which the shared route
//! loads under a strict content security policy. Validation happens at write time; what the
//! public route serves was already validated once, by this module.

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// The longest page name (the library label).
pub const PAGE_NAME_MAX: usize = 120;
/// The longest page title.
pub const PAGE_TITLE_MAX: usize = 200;
/// The most blocks one page may carry.
pub const PAGE_MAX_BLOCKS: usize = 50;
/// The longest text block.
pub const PAGE_BLOCK_TEXT_MAX: usize = 10_000;
/// The most columns a table may have.
pub const PAGE_MAX_COLUMNS: usize = 12;
/// The most rows a table may carry.
pub const PAGE_MAX_ROWS: usize = 500;
/// The longest cell in a table.
pub const PAGE_CELL_MAX: usize = 500;
/// The longest chart point label.
pub const PAGE_POINT_LABEL_MAX: usize = 120;
/// The largest single chart value.
pub const PAGE_MAX_VALUE: f64 = 1e15;
/// The longest link label.
pub const PAGE_LINK_LABEL_MAX: usize = 200;
/// The longest data-URI image source.
pub const PAGE_DATA_IMAGE_MAX: usize = 512 * 1024;
/// The largest page source on the wire, as stored JSON.
pub const PAGE_SOURCE_MAX_BYTES: usize = 256 * 1024;

/// One bar of a bar chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartPoint {
    /// The bar's label.
    pub label: String,
    /// The bar's value; finite and bounded.
    pub value: f64,
}

/// One block of the page document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum PageBlock {
    /// Plain text, bounded.
    #[serde(rename = "text")]
    Text {
        /// The text itself.
        text: String,
    },
    /// A table with uniform rows.
    #[serde(rename = "table")]
    Table {
        /// The column headers.
        columns: Vec<String>,
        /// Each row has exactly `columns.len()` cells.
        rows: Vec<Vec<String>>,
    },
    /// A bar chart. One kind ships this revision.
    #[serde(rename = "chart")]
    Chart {
        /// The chart's caption.
        label: String,
        /// The points, in order.
        points: Vec<ChartPoint>,
    },
    /// A link. `href` is `https?://` or `mailto:` and nothing else.
    #[serde(rename = "link")]
    Link {
        /// The link's label.
        label: String,
        /// The target.
        href: String,
    },
    /// An image by URL or data URI.
    #[serde(rename = "image")]
    Image {
        /// `https://`, or `data:image/(png|jpeg|gif|webp);base64,…`.
        src: String,
        /// The alt text.
        alt: String,
    },
}

/// A validated page document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSource {
    /// The title rendered at the top and in the tab.
    pub title: String,
    /// The blocks, in order.
    pub blocks: Vec<PageBlock>,
}

/// The longest page name check.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the name is refused.
pub fn validate_page_name(name: &str) -> Result<(), DomainError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidName(
            "the page name must not be empty".to_owned(),
        ));
    }
    if trimmed.chars().count() > PAGE_NAME_MAX {
        return Err(DomainError::InvalidName(format!(
            "the page name is longer than {PAGE_NAME_MAX} characters"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(DomainError::InvalidName(
            "the page name must not contain control characters".to_owned(),
        ));
    }
    Ok(())
}

/// Whether `href` is a link a page may carry: `https?://` or `mailto:`.
fn href_ok(href: &str) -> bool {
    href.starts_with("https://") || href.starts_with("http://") || href.starts_with("mailto:")
}

/// Whether `src` is an image a page may carry.
fn image_src_ok(src: &str) -> bool {
    if src.starts_with("https://") || src.starts_with("http://") {
        return true;
    }
    // data: URIs only for the four raster types, only base64, bounded.
    for media in ["image/png", "image/jpeg", "image/gif", "image/webp"] {
        let prefix = format!("data:{media};base64,");
        if let Some(rest) = src.strip_prefix(&prefix) {
            return !rest.is_empty()
                && rest.len() <= PAGE_DATA_IMAGE_MAX
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b));
        }
    }
    false
}

/// Validates a page source from its stored JSON shape.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] with the reason when any block is refused.
#[allow(clippy::too_many_lines)] // the block validator is the contract, in one place
pub fn validate_page_source(value: &serde_json::Value) -> Result<PageSource, DomainError> {
    let raw = serde_json::to_string(value)
        .map_err(|_| DomainError::InvalidName("the page source does not serialise".to_owned()))?;
    if raw.len() > PAGE_SOURCE_MAX_BYTES {
        return Err(DomainError::InvalidName(format!(
            "the page source is larger than {PAGE_SOURCE_MAX_BYTES} bytes"
        )));
    }
    let source: PageSource = serde_json::from_value(value.clone()).map_err(|_| {
        DomainError::InvalidName("the page source does not match the block schema".to_owned())
    })?;
    let title = source.title.trim();
    if title.is_empty() {
        return Err(DomainError::InvalidName(
            "the page needs a title".to_owned(),
        ));
    }
    if title.chars().count() > PAGE_TITLE_MAX {
        return Err(DomainError::InvalidName(format!(
            "the page title is longer than {PAGE_TITLE_MAX} characters"
        )));
    }
    if source.blocks.len() > PAGE_MAX_BLOCKS {
        return Err(DomainError::InvalidName(format!(
            "the page has more than {PAGE_MAX_BLOCKS} blocks"
        )));
    }
    for (index, block) in source.blocks.iter().enumerate() {
        match block {
            PageBlock::Text { text } => {
                if text.chars().count() > PAGE_BLOCK_TEXT_MAX {
                    return Err(DomainError::InvalidName(format!(
                        "the text block {index} is longer than {PAGE_BLOCK_TEXT_MAX} characters"
                    )));
                }
            }
            PageBlock::Table { columns, rows } => {
                if columns.is_empty() || columns.len() > PAGE_MAX_COLUMNS {
                    return Err(DomainError::InvalidName(format!(
                        "the table block {index} must have 1..={PAGE_MAX_COLUMNS} columns"
                    )));
                }
                if columns.iter().any(|c| c.chars().count() > PAGE_CELL_MAX) {
                    return Err(DomainError::InvalidName(format!(
                        "a column header in block {index} is too long"
                    )));
                }
                if rows.len() > PAGE_MAX_ROWS {
                    return Err(DomainError::InvalidName(format!(
                        "the table block {index} has more than {PAGE_MAX_ROWS} rows"
                    )));
                }
                for row in rows {
                    if row.len() != columns.len() {
                        return Err(DomainError::InvalidName(format!(
                            "a row in block {index} does not match {columns_len} columns",
                            columns_len = columns.len()
                        )));
                    }
                    if row.iter().any(|cell| cell.chars().count() > PAGE_CELL_MAX) {
                        return Err(DomainError::InvalidName(format!(
                            "a cell in block {index} is longer than {PAGE_CELL_MAX} characters"
                        )));
                    }
                }
            }
            PageBlock::Chart { label, points } => {
                if label.chars().count() > PAGE_POINT_LABEL_MAX {
                    return Err(DomainError::InvalidName(format!(
                        "the chart block {index} has a label longer than {PAGE_POINT_LABEL_MAX}"
                    )));
                }
                if points.is_empty() {
                    return Err(DomainError::InvalidName(format!(
                        "the chart block {index} needs at least one point"
                    )));
                }
                for point in points {
                    if point.label.chars().count() > PAGE_POINT_LABEL_MAX {
                        return Err(DomainError::InvalidName(format!(
                            "a point in block {index} has a label longer than {PAGE_POINT_LABEL_MAX}"
                        )));
                    }
                    if !point.value.is_finite() || point.value.abs() > PAGE_MAX_VALUE {
                        return Err(DomainError::InvalidName(format!(
                            "a point in block {index} has a value outside the allowed range"
                        )));
                    }
                }
            }
            PageBlock::Link { label, href } => {
                if label.chars().count() > PAGE_LINK_LABEL_MAX {
                    return Err(DomainError::InvalidName(format!(
                        "the link block {index} has a label longer than {PAGE_LINK_LABEL_MAX}"
                    )));
                }
                if !href_ok(href) {
                    return Err(DomainError::InvalidName(format!(
                        "the link block {index} has a href outside https?:// and mailto:"
                    )));
                }
            }
            PageBlock::Image { src, alt } => {
                if !image_src_ok(src) {
                    return Err(DomainError::InvalidName(format!(
                        "the image block {index} has a src outside https?:// and bounded data: URIs"
                    )));
                }
                if alt.chars().count() > PAGE_CELL_MAX {
                    return Err(DomainError::InvalidName(format!(
                        "the image block {index} has an alt longer than {PAGE_CELL_MAX}"
                    )));
                }
            }
        }
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::{PAGE_MAX_BLOCKS, validate_page_source};

    fn doc() -> serde_json::Value {
        serde_json::json!({
            "title": "Weekly report",
            "blocks": [{ "type": "text", "text": "All green." }],
        })
    }

    #[test]
    fn req_037_a_minimal_document_validates() {
        let source = validate_page_source(&doc()).expect("the document validates");
        assert_eq!(source.title, "Weekly report");
        assert_eq!(source.blocks.len(), 1);
    }

    #[test]
    fn req_037_a_document_with_every_block_kind_validates() {
        let value = serde_json::json!({
            "title": "Dashboard",
            "blocks": [
                { "type": "text", "text": "Hello." },
                { "type": "table", "columns": ["a", "b"], "rows": [["1", "2"], ["3", "4"]] },
                { "type": "chart", "label": "signups", "points": [{ "label": "mon", "value": 12.0 }] },
                { "type": "link", "label": "Docs", "href": "https://example.com/docs" },
                { "type": "image", "src": "https://example.com/logo.png", "alt": "logo" },
                { "type": "image", "src": "data:image/png;base64,iVBORw0KGgo=", "alt": "pixel" },
            ],
        });
        let source = validate_page_source(&value).expect("the document validates");
        assert_eq!(source.blocks.len(), 6);
    }

    #[test]
    fn req_037_the_refusals_are_exact() {
        let cases = vec![
            serde_json::json!({"blocks": [{"type":"text","text":"x"}]}), // no title
            serde_json::json!({"title": "", "blocks": []}),              // empty title
            serde_json::json!({"title": "t", "blocks": [{"type":"text","text":"x", "extra": 1}]}), // unknown field
            serde_json::json!({"title": "t", "blocks": [{"type":"video"}]}), // unknown block
            serde_json::json!({"title": "t", "blocks": [{"type":"link","label":"l","href":"javascript:alert(1)"}]}),
            serde_json::json!({"title": "t", "blocks": [{"type":"image","src":"data:text/html;base64,PGI+","alt":"x"}]}),
            serde_json::json!({"title": "t", "blocks": [{"type":"chart","label":"c","points":[{"label":"p","value":1e308}]}]}),
            serde_json::json!({"title": "t", "blocks": [{"type":"chart","label":"c","points":[{"label":"p","value":f64::INFINITY}]}]}),
            serde_json::json!({"title": "t", "blocks": [{"type":"table","columns":["a"],"rows":[["1","2"]]}]}),
        ];
        for (index, case) in cases.into_iter().enumerate() {
            assert!(
                validate_page_source(&case).is_err(),
                "case {index} is refused"
            );
        }
    }

    #[test]
    fn req_037_the_block_cap_is_enforced() {
        let blocks = vec![serde_json::json!({"type":"text","text":"x"}); PAGE_MAX_BLOCKS + 1];
        let value = serde_json::json!({ "title": "t", "blocks": blocks });
        assert!(validate_page_source(&value).is_err());
    }
}
