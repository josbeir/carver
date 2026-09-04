use std::fs;

use super::install_syntax_assets;

#[test]
fn syntax_assets_should_replace_stale_embedded_grammar() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let syntax_dir = install_syntax_assets(directory.path())?;
    fs::write(syntax_dir.join("carve.lang"), "stale")?;

    install_syntax_assets(directory.path())?;

    assert!(fs::read_to_string(syntax_dir.join("carve.lang"))?.contains("id=\"carve\""));
    Ok(())
}
