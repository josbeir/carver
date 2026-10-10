//! Editing prefixes, including incomplete items that intentionally have no AST node.
//!
//! CONTEXT: `GtkSourceView` only copies indentation; Markdown editor crates do not implement
//! Carve's dialects or item attributes. Keep the small lexical boundary here and reuse the
//! existing Carve parser/renderer for grammar validation and numbering, rather than adding
//! a second markup parser or a numeral crate with different ranges and grammar.

use carve::{BlockNode, OrderedListType};

const PROBE: &str = "carver-editor-item";

/// A borrowed physical-line prefix shared by source commands and keyboard editing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListPrefix<'a> {
    /// Quote prefixes and indentation before the list marker.
    pub container: &'a str,
    /// Authored bullet or ordered marker, excluding item attributes.
    pub marker: &'a str,
    /// Validated item attributes abutting the marker, including their braces.
    pub attributes: &'a str,
    /// Byte offset at which this item's text begins, relative to the physical line.
    pub content_start: usize,
    /// Visual indentation inside the surrounding quote, using Carve's tab stops.
    pub indent: usize,
    /// Visual column of continuation content inside the quote.
    pub content_column: usize,
    /// The complete quote prefix, without list indentation.
    pub quote: &'a str,
    separator: &'a str,
    task_separator: Option<&'a str>,
    ordered_type: Option<OrderedListType>,
    ordered: bool,
    ordinal: Option<usize>,
}

/// Failure to render a representable successor marker.
#[derive(Debug, thiserror::Error)]
pub enum ListPrefixError {
    /// The authored ordinal cannot be incremented.
    #[error("list ordinal is out of range")]
    OrdinalOverflow,
    /// Carve could not spell the synthetic list used for marker generation.
    #[error(transparent)]
    Render(#[from] carve::RenderCarveError),
    /// The renderer did not produce the expected standalone item.
    #[error("Carve did not produce a standalone list marker")]
    MissingMarker,
}

impl<'a> ListPrefix<'a> {
    /// Recognizes a list prefix, including an item with no text yet.
    ///
    /// `ordered_type` is the enclosing AST list's dialect; it disambiguates markers
    /// such as `i`, `v`, and `x`. Without it, Carve's standalone-marker rules apply.
    #[must_use]
    pub fn parse(line: &'a str, ordered_type: Option<OrderedListType>) -> Option<Self> {
        let (quote, rest) = quote_prefix(line);
        let indentation = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let start = quote.len() + indentation;
        let tail = line.get(start..)?;
        let marker_len = if tail.starts_with(['-', '*']) {
            1
        } else {
            let number = tail.bytes().take_while(u8::is_ascii_alphanumeric).count();
            match tail.as_bytes().get(number) {
                Some(b'.') => number + 1,
                Some(b')') if number > 0 => number + 1,
                _ => return None,
            }
        };
        let marker_end = start + marker_len;
        let attrs_end = attribute_end(line, marker_end)?;
        if line.as_bytes().get(attrs_end) != Some(&b' ') {
            return None;
        }
        let separator_end = whitespace_end(line, attrs_end);
        // Validate the marker and attributes with the real grammar, independently of the
        // unfinished body (which may be empty, a heading, or another block opener).
        let probe = format!("{}{PROBE}", &line[start..separator_end]);
        let document = carve::parse(&probe);
        let BlockNode::List(list) = document.children.first()? else {
            return None;
        };
        let task_end = separator_end.checked_add(4)?;
        let task_probe = line
            .get(separator_end..task_end)
            .filter(|prefix| {
                prefix.starts_with('[')
                    && prefix.as_bytes().get(2) == Some(&b']')
                    && prefix.ends_with(' ')
            })
            .map(|box_prefix| carve::parse(&format!("- {box_prefix}{PROBE}")));
        let task = task_probe.as_ref().is_some_and(|document| {
            matches!(document.children.first(), Some(BlockNode::List(list)) if list.items.first().is_some_and(|item| item.checked.is_some()))
        }) && !list.ordered;
        let content_start = if task {
            whitespace_end(line, task_end - 1)
        } else {
            separator_end
        };
        let dialect = list.ol_type.map(|dialect| ordered_type.unwrap_or(dialect));
        let marker = &line[start..marker_end];
        let ordinal = if list.ordered {
            marker_ordinal(&marker[..marker.len() - 1], dialect, list.start)
        } else {
            None
        };
        let indent = columns(&line[quote.len()..start]);
        let separator = &line[attrs_end..separator_end];
        let content_column = advance_columns(separator, indent + marker.len());
        Some(Self {
            container: &line[..start],
            marker,
            attributes: &line[marker_end..attrs_end],
            content_start,
            indent,
            content_column,
            quote,
            separator,
            task_separator: task.then(|| &line[task_end - 1..content_start]),
            ordered_type: dialect,
            ordered: list.ordered,
            ordinal,
        })
    }

    /// Whether the prefix carries a task checkbox, including custom Carve task states.
    #[must_use]
    pub fn is_task(&self) -> bool {
        self.task_separator.is_some()
    }

    /// Whether the prefix is an ordered-list marker.
    #[must_use]
    pub fn is_ordered(&self) -> bool {
        self.ordered
    }

    /// Produces the next item's prefix, preserving dialect and spacing, without attributes.
    ///
    /// # Errors
    /// Returns an error when the ordinal overflows or Carve cannot render the marker.
    pub fn successor(&self) -> Result<String, ListPrefixError> {
        let marker = if self.ordered && self.marker != "." {
            self.render_ordered(
                self.ordinal
                    .and_then(|n| n.checked_add(1))
                    .ok_or(ListPrefixError::OrdinalOverflow)?,
            )?
        } else {
            self.marker.to_owned()
        };
        let task = self
            .task_separator
            .map_or_else(String::new, |separator| format!("[ ]{separator}"));
        Ok(format!(
            "{}{marker}{}{task}",
            self.container, self.separator
        ))
    }

    fn render_ordered(&self, ordinal: usize) -> Result<String, ListPrefixError> {
        if self.ordered_type.is_none() {
            // Decimal spelling needs no grammar conversion. Avoid the renderer's internal
            // next-counter increment overflowing after the largest representable item.
            let delimiter = self
                .marker
                .chars()
                .last()
                .ok_or(ListPrefixError::MissingMarker)?;
            return Ok(format!("{ordinal}{delimiter}"));
        }
        // Two consecutive items retain Roman/alpha dialect at ambiguous single letters.
        // Only this scratch document is serialized; authored note content is never formatted.
        let mut document = carve::parse(&format!(". {PROBE}\n. {PROBE}"));
        let Some(BlockNode::List(list)) = document.children.first_mut() else {
            return Err(ListPrefixError::MissingMarker);
        };
        list.bare_marker = false;
        list.start = Some(ordinal - 1);
        list.ol_type = self.ordered_type;
        list.delim = self.marker.chars().last();
        let rendered = carve::render_carve(&document)?;
        rendered
            .lines()
            .rev()
            .find_map(|line| line.strip_suffix(PROBE))
            .map(|prefix| prefix.trim().to_owned())
            .ok_or(ListPrefixError::MissingMarker)
    }
}

/// Returns the literal quote prefix and remaining text of a physical line.
#[must_use]
pub fn quote_prefix(line: &str) -> (&str, &str) {
    let mut end = 0;
    loop {
        let whitespace = whitespace_end(line, end);
        if line.as_bytes().get(whitespace) != Some(&b'>') {
            break;
        }
        end = whitespace + 1;
        if line.as_bytes().get(end) == Some(&b' ') {
            end += 1;
        }
    }
    line.split_at(end)
}

/// Computes visual columns with Carve's four-column tab stops.
#[must_use]
pub fn columns(text: &str) -> usize {
    advance_columns(text, 0)
}

fn advance_columns(text: &str, start: usize) -> usize {
    text.chars().fold(start, |column, ch| {
        if ch == '\t' {
            column + 4 - column % 4
        } else {
            column + 1
        }
    })
}

fn whitespace_end(line: &str, start: usize) -> usize {
    start
        + line[start..]
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count()
}

fn attribute_end(line: &str, start: usize) -> Option<usize> {
    if line.as_bytes().get(start) != Some(&b'{') {
        return Some(start);
    }
    let mut quote = None;
    let mut escaped = false;
    for (offset, ch) in line[start + 1..].char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if quote == Some(ch) {
            quote = None;
        } else if quote.is_none() {
            if matches!(ch, '\'' | '"') {
                quote = Some(ch);
            } else if ch == '}' {
                return Some(start + 2 + offset);
            }
        }
    }
    None
}

fn marker_ordinal(
    marker: &str,
    dialect: Option<OrderedListType>,
    parsed: Option<usize>,
) -> Option<usize> {
    if marker.is_empty() {
        return None;
    }
    match dialect {
        None => marker.parse().ok(),
        Some(OrderedListType::LowerAlpha | OrderedListType::UpperAlpha) => {
            let letter = marker.as_bytes().first()?.to_ascii_lowercase();
            (marker.len() == 1 && letter.is_ascii_lowercase())
                .then(|| usize::from(letter - b'a' + 1))
        }
        Some(OrderedListType::LowerRoman | OrderedListType::UpperRoman) => {
            // The parser defaults most standalone single letters to alpha. The enclosing
            // list already resolved their dialect, so decode only these seven symbols here.
            if marker.len() == 1 {
                match marker.to_ascii_lowercase().as_str() {
                    "i" => Some(1),
                    "v" => Some(5),
                    "x" => Some(10),
                    "l" => Some(50),
                    "c" => Some(100),
                    "d" => Some(500),
                    "m" => Some(1000),
                    _ => None,
                }
            } else {
                Some(parsed.unwrap_or(1))
            }
        }
    }
}

#[cfg(test)]
mod tests;
