use super::*;

#[test]
fn language_id_should_match_the_frontmatter_format_token() {
    assert_eq!(language_id(FrontmatterFormat::Yaml), "yaml");
    assert_eq!(language_id(FrontmatterFormat::Toml), "toml");
    assert_eq!(language_id(FrontmatterFormat::Json), "json");
}
