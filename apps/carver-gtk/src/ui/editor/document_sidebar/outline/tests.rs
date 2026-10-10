use super::*;

fn heading(label: &str, level: u8) -> HeadingOccurrence {
    HeadingOccurrence {
        label: label.to_owned(),
        level,
        range: 0..10,
        text_start: 3,
    }
}

#[test]
fn heading_structure_should_match_when_only_source_positions_change() {
    let previous = vec![heading("Introduction", 1), heading("Details", 2)];
    let mut current = previous.clone();
    current[1].range = 100..110;
    current[1].text_start = 103;
    assert!(heading_structure_matches(&previous, &current));
}

#[test]
fn heading_structure_should_change_when_labels_levels_or_count_change() {
    let previous = vec![heading("Introduction", 1), heading("Details", 2)];
    for current in [
        vec![heading("Renamed", 1), heading("Details", 2)],
        vec![heading("Introduction", 1), heading("Details", 3)],
        vec![heading("Introduction", 1)],
        vec![heading("Details", 2), heading("Introduction", 1)],
    ] {
        assert!(!heading_structure_matches(&previous, &current));
    }
    assert!(heading_structure_matches(&[], &[]));
}
