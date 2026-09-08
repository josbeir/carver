//! Marks semantic headings at render time without trusting authored HTML attributes.
use carve::{BeforeRenderContext, BlockNode, CarveExtension, Document};

pub(super) struct HeadingProvenance(pub String);

impl CarveExtension for HeadingProvenance {
    fn name(&self) -> &'static str {
        "carver-heading-provenance"
    }

    fn before_render(&self, mut doc: Document, _: &BeforeRenderContext<'_>) -> Document {
        self.annotate(&mut doc.children);
        doc
    }
}

impl HeadingProvenance {
    fn annotate(&self, blocks: &mut [BlockNode]) {
        for block in blocks {
            match block {
                BlockNode::Heading(heading) => {
                    heading
                        .attrs
                        .get_or_insert_default()
                        .key_values
                        .insert("data-carver-heading".to_owned(), self.0.clone());
                }
                BlockNode::List(list) => {
                    for item in &mut list.items {
                        self.annotate(&mut item.children);
                    }
                }
                BlockNode::DefinitionList(list) => {
                    for item in &mut list.items {
                        for definition in &mut item.definitions {
                            self.annotate(&mut definition.children);
                        }
                    }
                }
                BlockNode::BlockQuote(node) => self.annotate(&mut node.children),
                BlockNode::Admonition(node) => self.annotate(&mut node.children),
                BlockNode::Div(node) => self.annotate(&mut node.children),
                BlockNode::LineBlock(node) => self.annotate(&mut node.children),
                BlockNode::FigureGroup(node) => self.annotate(&mut node.children),
                BlockNode::Extension(node) => self.annotate(&mut node.children),
                _ => {}
            }
        }
    }
}
