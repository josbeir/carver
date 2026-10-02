use super::{
    NOTE_LINK_SCHEME, extract_note_link_targets, extract_note_links, note_link_destination,
    parse_note_link_destination, rewrite_note_link_destinations,
};
use crate::NoteId;
use uuid::Uuid;

const TARGET: &str = "0192f3a1-7c4b-7d2e-9f10-3a5b6c7d8e9f";
const OTHER: &str = "0192f3a1-7c4b-7d2e-9f10-3a5b6c7d8ea0";

fn note_id(value: &str) -> NoteId {
    NoteId::from_uuid(Uuid::parse_str(value).unwrap_or_default())
}

#[test]
fn destination_should_round_trip_a_note_id() {
    let id = note_id(TARGET);
    let destination = note_link_destination(id);
    assert_eq!(destination, format!("{NOTE_LINK_SCHEME}{TARGET}"));
    assert_eq!(parse_note_link_destination(&destination), Some(id));
}

#[test]
fn destination_should_reject_external_and_malformed_values() {
    assert_eq!(parse_note_link_destination("https://example.com"), None);
    assert_eq!(parse_note_link_destination("carver:note/not-a-uuid"), None);
    assert_eq!(
        parse_note_link_destination(&format!("{NOTE_LINK_SCHEME}{TARGET}#heading")),
        None
    );
    assert_eq!(parse_note_link_destination("assets/image.png"), None);
}

#[test]
fn inline_link_should_extract_target_label_and_range() {
    let source = format!("See [Release]({NOTE_LINK_SCHEME}{TARGET}) now.");
    let links = extract_note_links(&source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, note_id(TARGET));
    assert_eq!(links[0].label, "Release");
    assert!(links[0].range.start < links[0].range.end);
}

#[test]
fn reference_forms_should_resolve_to_the_same_target() {
    let source = format!(
        "[full][ref]\n\n[collapsed][]\n\n[shortcut]\n\n[ref]: {NOTE_LINK_SCHEME}{TARGET}\n[collapsed]: {NOTE_LINK_SCHEME}{TARGET}\n[shortcut]: {NOTE_LINK_SCHEME}{TARGET}\n"
    );
    assert_eq!(extract_note_link_targets(&source), vec![note_id(TARGET)]);
}

#[test]
fn unresolved_reference_should_not_extract() {
    let source = "See [missing][absent] and [heading][].";
    assert_eq!(
        extract_note_links(source),
        [] as [crate::note_links::NoteLinkRef; 0]
    );
}

#[test]
fn heading_reference_should_not_extract() {
    let source = "## Section\n\nSee [Section][] for details.";
    assert_eq!(
        extract_note_links(source),
        [] as [crate::note_links::NoteLinkRef; 0]
    );
}

#[test]
fn external_and_asset_links_should_be_ignored() {
    let source =
        "A [site](https://example.com), an ![image](assets/pic.png), and a [file](assets/doc.pdf).";
    assert_eq!(
        extract_note_links(source),
        [] as [crate::note_links::NoteLinkRef; 0]
    );
}

#[test]
fn duplicate_links_should_dedupe_by_first_occurrence() {
    let source = format!(
        "[One]({NOTE_LINK_SCHEME}{TARGET}) and [Two]({NOTE_LINK_SCHEME}{TARGET}) and [Other]({NOTE_LINK_SCHEME}{OTHER})"
    );
    let links = extract_note_links(&source);
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].label, "One");
    assert_eq!(links[1].target, note_id(OTHER));
}

#[test]
fn nested_links_should_be_found_in_emphasis_and_lists() {
    let source = format!(
        "*emph [link]({NOTE_LINK_SCHEME}{TARGET})*\n\n- item [nested]({NOTE_LINK_SCHEME}{OTHER})\n"
    );
    let targets = extract_note_link_targets(&source);
    assert_eq!(targets, vec![note_id(TARGET), note_id(OTHER)]);
}

#[test]
fn self_and_duplicate_targets_should_collapse() {
    let source = format!("[a]({NOTE_LINK_SCHEME}{TARGET}) [b]({NOTE_LINK_SCHEME}{TARGET})");
    assert_eq!(extract_note_link_targets(&source), vec![note_id(TARGET)]);
}

#[test]
fn rewriting_destinations_should_remove_the_carver_scheme() {
    let source = format!("See [A]({NOTE_LINK_SCHEME}{TARGET}) and [B]({NOTE_LINK_SCHEME}{OTHER}).");
    let mut replacements = std::collections::BTreeMap::new();
    replacements.insert(note_id(TARGET), "release-checklist".to_owned());
    let rewritten = rewrite_note_link_destinations(&source, &replacements);
    assert!(rewritten.contains("release-checklist"));
    assert!(!rewritten.contains(NOTE_LINK_SCHEME));
    // An unresolved target falls back to its bare identifier rather than the scheme.
    assert!(rewritten.contains(OTHER));
}

#[test]
fn rewriting_should_match_uppercase_and_unhyphenated_uuid_spellings() {
    let uppercase = TARGET.to_uppercase();
    let simple = TARGET.replace('-', "");
    let source =
        format!("See [A]({NOTE_LINK_SCHEME}{uppercase}) and [B]({NOTE_LINK_SCHEME}{simple}).\n");
    let mut replacements = std::collections::BTreeMap::new();
    replacements.insert(note_id(TARGET), "release-checklist".to_owned());
    let rewritten = rewrite_note_link_destinations(&source, &replacements);
    assert!(!rewritten.contains(NOTE_LINK_SCHEME));
    assert_eq!(rewritten.matches("release-checklist").count(), 2);
}

#[test]
fn rewriting_should_leave_a_literal_scheme_in_code_untouched() {
    let source = format!("See [A]({NOTE_LINK_SCHEME}{TARGET}) and `{NOTE_LINK_SCHEME}{TARGET}`.\n");
    let mut replacements = std::collections::BTreeMap::new();
    replacements.insert(note_id(TARGET), "release-checklist".to_owned());
    let rewritten = rewrite_note_link_destinations(&source, &replacements);
    assert!(rewritten.contains("[A](release-checklist)"));
    assert!(rewritten.contains(&format!("`{NOTE_LINK_SCHEME}{TARGET}`")));
}

#[test]
fn rewriting_should_resolve_links_after_multibyte_text() {
    // Carve positions are codepoint offsets, so a multibyte character before the link must not
    // shift the destination lookup into the middle of a UTF-8 sequence.
    let source = format!(
        "🚀 emoji 👋 then [A]({NOTE_LINK_SCHEME}{TARGET}) and [B][ref].\n\n[ref]: {NOTE_LINK_SCHEME}{OTHER}\n"
    );
    let mut replacements = std::collections::BTreeMap::new();
    replacements.insert(note_id(TARGET), "release-checklist".to_owned());
    replacements.insert(note_id(OTHER), "roadmap".to_owned());
    let rewritten = rewrite_note_link_destinations(&source, &replacements);
    assert!(rewritten.contains("[A](release-checklist)"));
    assert!(rewritten.contains("[ref]: roadmap"));
    assert!(!rewritten.contains(NOTE_LINK_SCHEME));
}

#[test]
fn rewriting_a_reference_definition_should_resolve_its_links() {
    let source = format!("See [A][ref].\n\n[ref]: {NOTE_LINK_SCHEME}{TARGET}\n");
    let mut replacements = std::collections::BTreeMap::new();
    replacements.insert(note_id(TARGET), "release-checklist".to_owned());
    let rewritten = rewrite_note_link_destinations(&source, &replacements);
    assert!(rewritten.contains("[ref]: release-checklist"));
    assert!(!rewritten.contains(NOTE_LINK_SCHEME));
}
