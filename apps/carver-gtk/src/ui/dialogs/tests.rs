use carver_config::DocumentWidth;

use super::{document_width_from_index, document_width_index, line_height_percent};

#[test]
fn document_width_indices_should_round_trip_each_option() {
    for (width, index) in [
        (DocumentWidth::Narrow, 0),
        (DocumentWidth::Comfortable, 1),
        (DocumentWidth::Wide, 2),
        (DocumentWidth::Full, 3),
    ] {
        assert_eq!(document_width_index(width), index);
        assert_eq!(document_width_from_index(index), width);
    }
    assert_eq!(document_width_from_index(99), DocumentWidth::Comfortable);
}

#[test]
fn line_height_percent_should_round_and_clamp_input() {
    assert_eq!(line_height_percent(1.726), 173);
    assert_eq!(line_height_percent(0.5), 100);
    assert_eq!(line_height_percent(3.0), 250);
    assert_eq!(line_height_percent(f64::NAN), 155);
}
