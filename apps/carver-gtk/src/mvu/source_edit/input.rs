//! Pure structural keyboard input. GTK only classifies and dispatches these instructions.

use carver_domain::source_analysis::{
    ListPrefix, ListPrefixError, SourceAnalysis, quote_prefix, quote_prefix_at_depth,
};

use super::{SourceEdit, character_offset_at_byte, character_to_byte};
use std::ops::Range;

/// A structural editing gesture in the canonical source projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceInput {
    /// Split an item or continue/exit a quote.
    Enter,
    /// Nest items beneath the preceding sibling.
    IndentList,
    /// Lift items to the enclosing list level.
    OutdentList,
}

/// Whether structural input replaces source, is a consumed no-op, or uses GTK's default.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceInputOutcome {
    /// A canonical edit with its restored character-based selection.
    Edit(SourceEdit),
    /// A recognized list operation that cannot change nesting at this position.
    Noop,
    /// Ordinary editing owns this input.
    Native,
}

/// Decorative guidance for the active unfinished source marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourcePlaceholder {
    /// An unfinished bullet or ordered list item.
    ListItem,
    /// An unfinished task, including Carve's custom checkbox states.
    Task,
}

impl SourcePlaceholder {
    /// Classifies a bare marker at the end cursor without changing canonical source.
    #[must_use]
    pub fn at(source: &str, analysis: &SourceAnalysis, selection: Range<usize>) -> Option<Self> {
        let line = Line::at(source, selection.start)?;
        Self::at_line(line.text, analysis, line.range, selection)
    }

    /// Classifies one physical line using absolute character offsets from the shared analysis.
    #[must_use]
    pub(crate) fn at_line(
        text: &str,
        analysis: &SourceAnalysis,
        line: Range<usize>,
        selection: Range<usize>,
    ) -> Option<Self> {
        if selection.start != selection.end
            || selection.start != line.end
            || analysis.protects_editing(selection)
        {
            return None;
        }
        let prefix = ListPrefix::parse(text, None)?;
        if !text.get(prefix.content_start..)?.trim().is_empty() {
            return None;
        }
        Some(if prefix.is_task() {
            Self::Task
        } else {
            Self::ListItem
        })
    }
}

struct Line<'a> {
    text: &'a str,
    range: Range<usize>,
}

impl<'a> Line<'a> {
    fn at(source: &'a str, offset: usize) -> Option<Self> {
        let byte = character_to_byte(source, offset)?;
        let start = source[..byte].rfind('\n').map_or(0, |at| at + 1);
        let end = source[byte..]
            .find('\n')
            .map_or(source.len(), |at| byte + at);
        Some(Self {
            text: &source[start..end],
            range: character_offset_at_byte(source, start)?..character_offset_at_byte(source, end)?,
        })
    }
}

impl SourceInput {
    /// Checks whether the source adapter should consume this gesture.
    ///
    /// The query shares the planner's context rules and never clones or edits the document.
    #[must_use]
    pub fn accepts(self, source: &str, analysis: &SourceAnalysis, selection: Range<usize>) -> bool {
        if analysis.protects_editing(selection.clone()) {
            return false;
        }
        let Some(line) = Line::at(source, selection.start) else {
            return false;
        };
        match self {
            Self::Enter => enter_prefix(source, analysis, &line, &selection).map_or_else(
                || quote_enter(&line, &selection),
                |prefix| prefix.successor().is_ok(),
            ),
            Self::IndentList | Self::OutdentList => {
                nesting_context(source, analysis, &selection).is_some()
            }
        }
    }
}

impl SourceEdit {
    /// Plans structural input against the current immutable source analysis.
    ///
    /// # Errors
    /// Returns a typed error when Carve cannot produce a representable successor marker.
    pub fn plan_input(
        source: &str,
        analysis: &SourceAnalysis,
        selection: Range<usize>,
        input: SourceInput,
    ) -> Result<SourceInputOutcome, ListPrefixError> {
        if !input.accepts(source, analysis, selection.clone()) {
            return Ok(SourceInputOutcome::Native);
        }
        match input {
            SourceInput::Enter => enter(source, analysis, selection),
            SourceInput::IndentList | SourceInput::OutdentList => {
                Ok(nest(source, analysis, selection, input))
            }
        }
    }

    fn input_replacement(source: &str, range: Range<usize>, text: &str) -> Self {
        let cursor = range.start + text.chars().count();
        let mut edit = Self::new(source.to_owned(), range.clone());
        edit.replace(range, text, 0);
        edit.selection = cursor..cursor;
        edit
    }
}

fn item_at(source: &str, analysis: &SourceAnalysis, offset: usize) -> Option<usize> {
    let line = Line::at(source, offset)?;
    analysis
        .list_items()
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, item)| (item.marker_line.start == line.range.start).then_some(index))
        .or_else(|| analysis.list_item_for(offset..offset))
}

fn item_prefix<'a>(
    source: &'a str,
    analysis: &SourceAnalysis,
    index: usize,
) -> Option<ListPrefix<'a>> {
    let item = analysis.list_items().get(index)?;
    let line = Line::at(source, item.range.start)?;
    ListPrefix::parse(line.text, item.ordered_type)
}

pub(super) fn hard_break_prefix(
    source: &str,
    analysis: &SourceAnalysis,
    selection: &Range<usize>,
) -> String {
    if analysis.protects_editing(selection.clone()) {
        return String::new();
    }
    if let Some(prefix) = item_at(source, analysis, selection.start)
        .and_then(|index| item_prefix(source, analysis, index))
    {
        return format!("{}{}", prefix.quote, " ".repeat(prefix.content_column));
    }
    Line::at(source, selection.start)
        .map_or_else(String::new, |line| quote_prefix(line.text).0.to_owned())
}

fn enter_prefix<'a>(
    source: &'a str,
    analysis: &SourceAnalysis,
    line: &Line<'a>,
    selection: &Range<usize>,
) -> Option<ListPrefix<'a>> {
    let index = item_at(source, analysis, selection.start);
    let dialect = index
        .and_then(|index| analysis.list_items().get(index))
        .filter(|item| item.marker_line.start == line.range.start)
        .and_then(|item| item.ordered_type);
    if let Some(prefix) = ListPrefix::parse(line.text, dialect) {
        let content_start = line.range.start + line.text[..prefix.content_start].chars().count();
        if selection.start < content_start || selection.end > line.range.end {
            return None;
        }
        return Some(prefix);
    }
    let index = index?;
    let item = &analysis.list_items()[index];
    if selection.end > item.range.end || selection.end > line.range.end {
        return None;
    }
    let context = analysis.context_for(selection.start.saturating_sub(1)..selection.start);
    if context.is_some_and(|context| {
        context.path().last() != Some(&carver_domain::source_analysis::SourceNodeKind::Paragraph)
    }) {
        return None;
    }
    item_prefix(source, analysis, index)
}

fn quote_enter(line: &Line<'_>, selection: &Range<usize>) -> bool {
    let (quote, _) = quote_prefix(line.text);
    !quote.is_empty()
        && selection.start >= line.range.start + quote.chars().count()
        && selection.end <= line.range.end
        && ListPrefix::parse(line.text, None).is_none()
}

fn enter(
    source: &str,
    analysis: &SourceAnalysis,
    selection: Range<usize>,
) -> Result<SourceInputOutcome, ListPrefixError> {
    let Some(line) = Line::at(source, selection.start) else {
        return Ok(SourceInputOutcome::Native);
    };
    if let Some(prefix) = enter_prefix(source, analysis, &line, &selection) {
        let empty = selection.is_empty()
            && ListPrefix::parse(line.text, None).is_some()
            && line
                .text
                .get(prefix.content_start..)
                .is_some_and(|text| text.trim().is_empty());
        if empty {
            let parent = empty_parent(source, analysis, &line, &prefix);
            let text =
                parent.map_or_else(|| Ok(prefix.quote.to_owned()), |parent| parent.successor())?;
            return Ok(SourceInputOutcome::Edit(SourceEdit::input_replacement(
                source, line.range, &text,
            )));
        }
        return Ok(SourceInputOutcome::Edit(SourceEdit::input_replacement(
            source,
            selection,
            &format!("\n{}", prefix.successor()?),
        )));
    }
    let (quote, rest) = quote_prefix(line.text);
    let text = if rest.trim().is_empty() && selection.is_empty() {
        let last = quote.rfind('>').unwrap_or_default();
        if quote[..last].contains('>') {
            quote[..last].to_owned()
        } else {
            String::new()
        }
    } else {
        format!("\n{quote}")
    };
    let range = if rest.trim().is_empty() && selection.is_empty() {
        line.range
    } else {
        selection
    };
    Ok(SourceInputOutcome::Edit(SourceEdit::input_replacement(
        source, range, &text,
    )))
}

fn empty_parent<'a>(
    source: &'a str,
    analysis: &SourceAnalysis,
    line: &Line<'_>,
    prefix: &ListPrefix<'_>,
) -> Option<ListPrefix<'a>> {
    analysis
        .list_items()
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, item)| {
            if item.range.start >= line.range.start
                || !adjacent(source, item.range.end, line.range.start)
            {
                return None;
            }
            let parent = item_prefix(source, analysis, index)?;
            (parent.quote == prefix.quote && parent.indent < prefix.indent).then_some(parent)
        })
}

fn adjacent(source: &str, end: usize, start: usize) -> bool {
    if end >= start {
        return true;
    }
    let Some(end) = character_to_byte(source, end) else {
        return false;
    };
    let Some(start) = character_to_byte(source, start) else {
        return false;
    };
    source[end..start].trim().is_empty()
}

struct Targets {
    range: Range<usize>,
    first: usize,
}

fn targets(source: &str, analysis: &SourceAnalysis, selection: &Range<usize>) -> Option<Targets> {
    let first = item_at(source, analysis, selection.start)?;
    let item = &analysis.list_items()[first];
    let end_line = Line::at(
        source,
        if selection.is_empty() {
            selection.end
        } else {
            selection.end.saturating_sub(1)
        },
    )?;
    let last = analysis
        .list_items()
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.list == item.list
                && candidate.range.start >= item.range.start
                && candidate.range.start <= end_line.range.end
        })
        .map(|(index, _)| index)
        .next_back()
        .unwrap_or(first);
    let last_item = &analysis.list_items()[last];
    let start = Line::at(source, item.range.start)?.range.start;
    let end = Line::at(source, last_item.range.end.saturating_sub(1))?
        .range
        .end;
    (selection.start >= start && selection.end <= end.saturating_add(1)).then_some(Targets {
        range: start..end,
        first,
    })
}

struct NestingContext<'a> {
    range: Range<usize>,
    prefix: ListPrefix<'a>,
    previous: Option<ListPrefix<'a>>,
    parent: Option<ListPrefix<'a>>,
}

fn nesting_context<'a>(
    source: &'a str,
    analysis: &SourceAnalysis,
    selection: &Range<usize>,
) -> Option<NestingContext<'a>> {
    let line = Line::at(source, selection.start)?;
    if let Some(prefix) = ListPrefix::parse(line.text, None)
        .filter(|prefix| line.text[prefix.content_start..].trim().is_empty())
    {
        if selection.end > line.range.end {
            return None;
        }
        let previous = analysis
            .list_items()
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, item)| {
                if item.range.start >= line.range.start
                    || !adjacent(source, item.range.end, line.range.start)
                {
                    return None;
                }
                let candidate = item_prefix(source, analysis, index)?;
                (candidate.indent == prefix.indent && candidate.quote == prefix.quote)
                    .then_some(candidate)
            });
        let parent = empty_parent(source, analysis, &line, &prefix);
        return Some(NestingContext {
            range: line.range,
            prefix,
            previous,
            parent,
        });
    }
    let targets = targets(source, analysis, selection)?;
    let first = &analysis.list_items()[targets.first];
    Some(NestingContext {
        range: targets.range,
        prefix: item_prefix(source, analysis, targets.first)?,
        previous: first
            .previous
            .and_then(|index| item_prefix(source, analysis, index)),
        parent: first
            .parent
            .and_then(|index| item_prefix(source, analysis, index)),
    })
}

fn nest(
    source: &str,
    analysis: &SourceAnalysis,
    selection: Range<usize>,
    input: SourceInput,
) -> SourceInputOutcome {
    let Some(context) = nesting_context(source, analysis, &selection) else {
        return SourceInputOutcome::Native;
    };
    let prefix = &context.prefix;
    let target = match input {
        SourceInput::IndentList => context.previous.map(|previous| previous.content_column),
        SourceInput::OutdentList => context.parent.map(|parent| parent.indent),
        SourceInput::Enter => None,
    };
    let Some(target) = target else {
        return SourceInputOutcome::Noop;
    };
    if target == prefix.indent {
        return SourceInputOutcome::Noop;
    }
    let Some(start_byte) = character_to_byte(source, context.range.start) else {
        return SourceInputOutcome::Native;
    };
    let Some(end_byte) = character_to_byte(source, context.range.end) else {
        return SourceInputOutcome::Native;
    };
    let mut replacement = String::new();
    let mut mapping = Vec::new();
    let mut old_offset = context.range.start;
    let mut new_offset = context.range.start;
    let quote_depth = prefix.quote.bytes().filter(|byte| *byte == b'>').count();
    for (index, line) in source[start_byte..end_byte].split('\n').enumerate() {
        if index > 0 {
            replacement.push('\n');
            old_offset += 1;
            new_offset += 1;
        }
        // Only enclosing quotes stay fixed; descendant block markers move with the item.
        let (quote, rest) = quote_prefix_at_depth(line, quote_depth);
        let indent_bytes = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let old_indent = carver_domain::source_analysis::columns(&rest[..indent_bytes]);
        let new_indent = if target > prefix.indent {
            old_indent + target - prefix.indent
        } else {
            old_indent.saturating_sub(prefix.indent - target)
        };
        let new_prefix = format!(
            "{quote}{}",
            resized_indent(&rest[..indent_bytes], new_indent)
        );
        let old_prefix_chars = quote.chars().count() + rest[..indent_bytes].chars().count();
        let new_prefix_chars = new_prefix.chars().count();
        let line_chars = line.chars().count();
        mapping.push((
            old_offset,
            new_offset,
            old_prefix_chars,
            new_prefix_chars,
            line_chars,
        ));
        replacement.push_str(&new_prefix);
        replacement.push_str(&rest[indent_bytes..]);
        old_offset += line_chars;
        new_offset += new_prefix_chars + line_chars - old_prefix_chars;
    }
    let mapped = |offset: usize| {
        mapping
            .iter()
            .find_map(|&(old, new, removed, inserted, length)| {
                (old <= offset && offset <= old + length)
                    .then(|| new + inserted + offset.saturating_sub(old + removed))
            })
            .unwrap_or(new_offset + offset.saturating_sub(old_offset))
    };
    let mut edit = SourceEdit::input_replacement(source, context.range, &replacement);
    edit.selection = mapped(selection.start)..mapped(selection.end);
    SourceInputOutcome::Edit(edit)
}

fn resized_indent(indent: &str, target: usize) -> String {
    let mut column = 0;
    let mut result = String::new();
    for ch in indent.chars() {
        let next = column + if ch == '\t' { 4 - column % 4 } else { 1 };
        if next > target {
            break;
        }
        result.push(ch);
        column = next;
    }
    result.push_str(&" ".repeat(target - column));
    result
}

#[cfg(test)]
mod tests;
