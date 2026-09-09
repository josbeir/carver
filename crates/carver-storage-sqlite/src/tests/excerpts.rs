use super::super::note_excerpt;

#[test]
fn excerpt_should_preserve_ascii_up_to_the_existing_limit() {
    for length in [0, 179, 180, 181] {
        assert_eq!(
            note_excerpt(&"a".repeat(length)),
            "a".repeat(length.min(180))
        );
    }
}

#[test]
fn excerpt_should_keep_a_flag_pair_at_the_limit() {
    let expected = format!("{}🇧🇪", "a".repeat(179));
    assert_eq!(note_excerpt(&format!("{expected}tail")), expected);
}
