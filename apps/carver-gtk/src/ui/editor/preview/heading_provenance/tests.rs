use super::HeadingProvenance;
use carve::{BlockExtension, BlockNode, Directive, Section};

fn heading() -> BlockNode {
    carve::parse("# Nested").children.remove(0)
}

fn assert_marked(block: &BlockNode) {
    let BlockNode::Heading(heading) = block else {
        panic!("heading fixture");
    };
    assert_eq!(
        heading
            .attrs
            .as_ref()
            .and_then(|attrs| attrs.key_values.get("data-carver-heading"))
            .map(String::as_str),
        Some("provenance")
    );
}

#[test]
fn provenance_should_mark_headings_inside_interchange_sections() {
    let mut blocks = [BlockNode::Section(Section {
        attrs: None,
        level: None,
        children: vec![heading()],
        pos: None,
    })];
    HeadingProvenance("provenance".into()).annotate(&mut blocks);
    let BlockNode::Section(section) = &blocks[0] else {
        panic!("section fixture");
    };
    assert_marked(&section.children[0]);
}

#[test]
fn provenance_should_mark_headings_inside_directives() {
    let mut blocks = [BlockNode::Directive(Directive {
        attrs: None,
        kind: "toc".into(),
        title: None,
        label: None,
        children: vec![heading()],
        pos: None,
    })];
    HeadingProvenance("provenance".into()).annotate(&mut blocks);
    let BlockNode::Directive(directive) = &blocks[0] else {
        panic!("directive fixture");
    };
    assert_marked(&directive.children[0]);
}

#[test]
fn provenance_should_mark_headings_inside_extension_fallbacks() {
    let mut blocks = [BlockNode::BlockExtension(BlockExtension {
        name: "org.example.heading".into(),
        version: None,
        fallback: Box::new(heading()),
        payload: None,
        attrs: None,
        pos: None,
    })];
    HeadingProvenance("provenance".into()).annotate(&mut blocks);
    let BlockNode::BlockExtension(extension) = &blocks[0] else {
        panic!("extension fixture");
    };
    assert_marked(&extension.fallback);
}

#[test]
fn provenance_should_mark_headings_inside_render_extension_carriers() {
    let mut blocks = [BlockNode::ExtensionCarrier(carve::ExtensionCarrier {
        name: "details".into(),
        attrs: None,
        children: vec![heading()],
        summary: None,
        label: None,
        pos: None,
    })];
    HeadingProvenance("provenance".into()).annotate(&mut blocks);
    let BlockNode::ExtensionCarrier(carrier) = &blocks[0] else {
        panic!("carrier fixture");
    };
    assert_marked(&carrier.children[0]);
}
