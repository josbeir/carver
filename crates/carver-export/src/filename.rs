//! Portable filename components shared by note exports and attachment previews.

/// Sanitizes a filename component for Unix and Windows within a UTF-8 byte budget.
///
/// Leading and trailing whitespace and dots are removed. Invalid filename characters
/// and reserved device names are replaced with `-`. An empty result is left to the
/// caller's naming policy. `max_bytes` is capped at 255; callers must reserve space
/// for any extension they append. This does not validate managed asset paths.
#[must_use]
pub fn sanitized_component(name: &str, max_bytes: usize) -> String {
    let options = sanitize_filename::Options {
        windows: true,
        truncate: false,
        replacement: "-",
    };
    let name = name.trim().trim_matches('.').trim();
    let mut name = sanitize_filename::sanitize_with_options(name, options.clone());
    name.truncate(name.floor_char_boundary(max_bytes.min(255)));
    // Truncating can expose a trailing dot or a reserved name (e.g. CONnection).
    sanitize_filename::sanitize_with_options(name.trim().trim_matches('.').trim(), options)
}

#[cfg(test)]
mod tests;
