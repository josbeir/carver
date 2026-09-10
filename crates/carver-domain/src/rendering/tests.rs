use super::*;

#[test]
fn enhanced_profile_should_place_contents_at_the_authored_location() {
    let source =
        "Intro\n\n{depth=\"2\"}\n::: toc\n:::\n\n# One\n\n## Same\n\n## Same\n\n### Hidden";
    let html = HtmlProfile::Enhanced
        .render_html(source, Mode::Interactive, &[])
        .unwrap_or_else(|error| panic!("{error}"))
        .value;
    assert!(
        matches!((html.find("Intro"), html.find("<nav")), (Some(intro), Some(nav)) if intro < nav)
    );
    assert!(html.contains("href=\"#One\""));
    assert!(html.contains("href=\"#Same\""));
    assert!(!html.contains("href=\"#Hidden\""));
    assert_eq!(html.matches("<nav").count(), 1);
}

#[test]
fn core_profile_should_keep_ordinary_containers_and_bare_urls() {
    let source = "::: toc\n:::\n\n::: details \"More\"\nBody\n:::\n\nhttps://example.com";
    let html = HtmlProfile::Core
        .render_html(source, Mode::Interactive, &[])
        .unwrap_or_else(|error| panic!("{error}"))
        .value;
    assert!(!html.contains("<nav"));
    assert!(!html.contains("<details"));
    assert!(!html.contains("<a "));
}

#[test]
fn enhanced_profile_should_only_autolink_web_urls_outside_code_and_links() {
    let source =
        "https://example.com. `https://code.test` [Explicit](https://link.test) user@example.com";
    let html = HtmlProfile::Enhanced
        .render_html(source, Mode::Interactive, &[])
        .unwrap_or_else(|error| panic!("{error}"))
        .value;
    assert!(html.contains("href=\"https://example.com\""));
    assert!(!html.contains("href=\"https://code.test\""));
    assert_eq!(html.matches("<a ").count(), 2);
}

#[test]
fn print_profile_should_expand_details_without_changing_authored_source() {
    let source = "::: details \"More\"\nHidden body\n:::";
    let html = HtmlProfile::Enhanced
        .render_html(source, Mode::Static, &[])
        .unwrap_or_else(|error| panic!("{error}"))
        .value;
    assert!(html.contains("<details open>"));
    assert!(html.contains("Hidden body"));
    assert!(
        !HtmlProfile::Enhanced
            .render_html(source, Mode::Interactive, &[])
            .unwrap_or_else(|error| panic!("{error}"))
            .value
            .contains("<details open>")
    );
}

#[test]
fn enhanced_render_should_report_dropped_raw_formats() {
    let result = HtmlProfile::Enhanced
        .render_html("```=latex\nopaque\n```", Mode::Interactive, &[])
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(result.total_losses, 1);
}
