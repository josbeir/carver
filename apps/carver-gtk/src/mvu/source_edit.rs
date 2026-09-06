//! Pure canonical-Carve editing commands used by the MVU reducer.

use std::ops::Range;

/// A source-formatting instruction supplied by a GTK adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceCommand {
    /// Toggle inline delimiters around the current selection.
    ToggleInline {
        /// Delimiter inserted before selected source.
        opening: String,
        /// Delimiter inserted after selected source.
        closing: String,
    },
    /// Toggle a fenced code block.
    ToggleCodeBlock,
    /// Toggle a list marker.
    ToggleList(String),
    /// Toggle an ordered list.
    ToggleOrderedList,
    /// Apply a heading level, where zero is ordinary text.
    SetHeading(u8),
    /// Insert a table at the selection.
    InsertTable {
        /// Number of table rows.
        rows: u8,
        /// Number of table columns.
        columns: u8,
        /// Whether the first row is a header.
        header: bool,
    },
    /// Update the direct image at the cursor.
    SetImageWidth(Option<u8>),
    /// Insert a direct Carve link.
    InsertLink {
        /// Link text to show in the document.
        text: String,
        /// Link target URI.
        destination: String,
    },
    /// Insert one explicit hard line break.
    InsertHardBreak,
}

/// The canonical source and character-based selection produced by a command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEdit {
    source: String,
    selection: Range<usize>,
}

impl SourceEdit {
    /// Applies `command` to a source snapshot and selection.
    #[must_use]
    pub fn apply(source: String, selection: Range<usize>, command: SourceCommand) -> Self {
        let mut edit = Self::new(source, selection);
        match command {
            SourceCommand::ToggleInline { opening, closing } => {
                edit.toggle_inline(&opening, &closing);
            }
            SourceCommand::ToggleCodeBlock => edit.toggle_code_block(),
            SourceCommand::ToggleList(prefix) => edit.toggle_list(&prefix),
            SourceCommand::ToggleOrderedList => edit.toggle_ordered_list(),
            SourceCommand::SetHeading(level) => edit.set_heading(level),
            SourceCommand::InsertTable {
                rows,
                columns,
                header,
            } => edit.insert_table(rows, columns, header),
            SourceCommand::SetImageWidth(width) => {
                let _ = edit.set_image_width(width);
            }
            SourceCommand::InsertLink { text, destination } => {
                edit.insert_link(&text, &destination);
            }
            SourceCommand::InsertHardBreak => edit.insert_hard_break(),
        }
        edit
    }

    fn new(source: String, selection: Range<usize>) -> Self {
        let length = source.chars().count();
        let start = selection.start.min(length);
        Self {
            source,
            selection: start..selection.end.clamp(start, length),
        }
    }

    /// Returns the transformed canonical source.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the transformed character-based selection.
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    fn toggle_inline(&mut self, opening: &str, closing: &str) {
        if self.selection.is_empty() {
            let cursor = self.selection.start;
            self.replace(cursor..cursor, &format!("{opening}{closing}"), 0);
            let cursor = cursor.saturating_add(opening.chars().count());
            self.selection = cursor..cursor;
            return;
        }
        let replacement = inline_replacement(&self.selected_text(), opening, closing);
        let length = replacement.chars().count();
        self.replace(self.selection.clone(), &replacement, length);
    }

    fn set_heading(&mut self, level: u8) {
        let prefix = if level == 0 {
            String::new()
        } else {
            format!("{} ", "#".repeat(usize::from(level.min(6))))
        };
        self.transform_lines(|line| heading_replacement(line, &prefix));
    }

    fn toggle_list(&mut self, prefix: &str) {
        let lines = self.selected_lines();
        let remove = lines.iter().all(|line| line.starts_with(prefix));
        self.transform_lines(|line| list_replacement(strip_list_marker(line), prefix, remove));
    }

    fn toggle_ordered_list(&mut self) {
        let lines = self.selected_lines();
        let remove = lines.iter().all(|line| ordered_list_item(line).is_some());
        let replacement = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let text = strip_list_marker(line);
                if remove {
                    text.to_owned()
                } else {
                    format!("{}. {text}", index + 1)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let range = self.selected_line_range();
        let length = replacement.chars().count();
        self.replace(range, &replacement, length);
    }

    fn toggle_code_block(&mut self) {
        if self.selection.is_empty() {
            self.toggle_inline("`", "`");
            return;
        }
        let selected = self.selected_text();
        let replacement = if selected.starts_with("```") && selected.ends_with("\n```") {
            selected
                .strip_prefix("```")
                .and_then(|text| text.strip_prefix('\n'))
                .and_then(|text| text.strip_suffix("\n```"))
                .unwrap_or(&selected)
                .to_owned()
        } else {
            format!("```\n{selected}\n```")
        };
        let length = replacement.chars().count();
        self.replace(self.selection.clone(), &replacement, length);
    }

    fn insert_hard_break(&mut self) {
        let range = self.selection.clone();
        self.replace(range.clone(), "\\\n", 0);
        let cursor = range.start.saturating_add(2);
        self.selection = cursor..cursor;
    }

    fn insert_link(&mut self, text: &str, destination: &str) {
        let markup = format!("[{text}]({destination})");
        let range = self.selection.clone();
        self.replace(range.clone(), &markup, 0);
        let cursor = range.start.saturating_add(markup.chars().count());
        self.selection = cursor..cursor;
    }

    fn insert_table(&mut self, rows: u8, columns: u8, header: bool) {
        if rows == 0 || columns == 0 {
            return;
        }
        let table = (0..rows)
            .map(|row| {
                (0..columns)
                    .map(|_| if header && row == 0 { "|= " } else { "| " })
                    .collect::<String>()
                    + "|"
            })
            .collect::<Vec<_>>()
            .join("\n");
        let markup = format!("\n{table}\n");
        let cursor = self.selection.end;
        self.replace(cursor..cursor, &markup, 0);
        let cursor = cursor.saturating_add(markup.chars().count());
        self.selection = cursor..cursor;
    }

    fn set_image_width(&mut self, width: Option<u8>) -> bool {
        let Some(cursor) = character_to_byte(&self.source, self.selection.end) else {
            return false;
        };
        let Some((start, end)) = image_span_at(&self.source, cursor) else {
            return false;
        };
        let replacement = image_with_width(&self.source[start..end], width);
        let (Some(start), Some(end)) = (
            character_offset_at_byte(&self.source, start),
            character_offset_at_byte(&self.source, end),
        ) else {
            return false;
        };
        let length = replacement.chars().count();
        self.replace(start..end, &replacement, length);
        true
    }

    fn selected_text(&self) -> String {
        let start =
            character_to_byte(&self.source, self.selection.start).unwrap_or(self.source.len());
        let end = character_to_byte(&self.source, self.selection.end).unwrap_or(self.source.len());
        self.source[start..end].to_owned()
    }

    fn selected_lines(&self) -> Vec<String> {
        let range = self.selected_line_range();
        self.source[character_to_byte(&self.source, range.start).unwrap_or(self.source.len())
            ..character_to_byte(&self.source, range.end).unwrap_or(self.source.len())]
            .split('\n')
            .map(str::to_owned)
            .collect()
    }

    fn transform_lines(&mut self, transform: impl Fn(&str) -> String) {
        let range = self.selected_line_range();
        let replacement = self
            .selected_lines()
            .iter()
            .map(|line| transform(line))
            .collect::<Vec<_>>()
            .join("\n");
        let length = replacement.chars().count();
        self.replace(range, &replacement, length);
    }

    fn selected_line_range(&self) -> Range<usize> {
        let start =
            character_to_byte(&self.source, self.selection.start).unwrap_or(self.source.len());
        let end = character_to_byte(&self.source, self.selection.end).unwrap_or(self.source.len());
        let start = self.source[..start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let end = self.source[end..]
            .find('\n')
            .map_or(self.source.len(), |index| end + index);
        character_offset_at_byte(&self.source, start).unwrap_or_default()
            ..character_offset_at_byte(&self.source, end).unwrap_or_default()
    }

    fn replace(&mut self, range: Range<usize>, replacement: &str, selected_length: usize) {
        let start = character_to_byte(&self.source, range.start).unwrap_or(self.source.len());
        let end = character_to_byte(&self.source, range.end).unwrap_or(self.source.len());
        self.source.replace_range(start..end, replacement);
        self.selection = range.start..range.start.saturating_add(selected_length);
    }
}

fn character_to_byte(source: &str, offset: usize) -> Option<usize> {
    source
        .char_indices()
        .nth(offset)
        .map(|(index, _)| index)
        .or_else(|| (offset == source.chars().count()).then_some(source.len()))
}
fn character_offset_at_byte(source: &str, offset: usize) -> Option<usize> {
    source.get(..offset).map(|prefix| prefix.chars().count())
}
fn strip_list_marker(line: &str) -> &str {
    ordered_list_item(line)
        .or_else(|| line.strip_prefix("- [ ] "))
        .or_else(|| line.strip_prefix("- "))
        .unwrap_or(line)
}
fn ordered_list_item(line: &str) -> Option<&str> {
    let (number, text) = line.split_once(". ")?;
    (!number.is_empty() && number.chars().all(|character| character.is_ascii_digit()))
        .then_some(text)
}
fn inline_replacement(selected: &str, opening: &str, closing: &str) -> String {
    selected
        .strip_prefix(opening)
        .and_then(|text| text.strip_suffix(closing))
        .map_or_else(|| format!("{opening}{selected}{closing}"), str::to_owned)
}
fn heading_replacement(line: &str, prefix: &str) -> String {
    format!("{prefix}{}", line.trim_start_matches('#').trim_start())
}
fn list_replacement(line: &str, prefix: &str, remove: bool) -> String {
    if remove {
        line.to_owned()
    } else {
        format!("{prefix}{line}")
    }
}
fn image_span_at(source: &str, cursor: usize) -> Option<(usize, usize)> {
    let start = source[..cursor].rfind("![")?;
    let mut end = source[start..].find(')')? + start + 1;
    if source[end..].starts_with('{') {
        end = source[end..].find('}')? + end + 1;
    }
    (cursor <= end).then_some((start, end))
}
fn image_with_width(image: &str, width: Option<u8>) -> String {
    let (base, attributes) = image
        .split_once('{')
        .map_or((image, None), |(base, attributes)| {
            (base, attributes.strip_suffix('}'))
        });
    let mut attributes = attributes
        .map(|attributes| {
            attributes
                .split_whitespace()
                .filter(|attribute| !attribute.starts_with("width="))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(width) = width {
        attributes.push(format!("width={width}%"));
    }
    if attributes.is_empty() {
        base.to_owned()
    } else {
        format!("{base}{{{}}}", attributes.join(" "))
    }
}
