use std::fs;

use super::*;

#[test]
fn clipboard_document_should_embed_a_small_managed_image() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("example.png"), [1_u8, 2, 3])?;

    let document = clipboard_document("![Diagram](assets/example.png)", Some(directory.path()))?;

    assert!(document.html.contains("src=\"data:image/png;base64,AQID\""));
    assert_eq!(document.plain_text.trim(), "Diagram");
    assert_eq!(document.omitted_images, 0);
    Ok(())
}

#[test]
fn clipboard_document_should_preserve_external_images() -> Result<(), Box<dyn std::error::Error>> {
    let document = clipboard_document("![Logo](https://example.test/logo.png)", None)?;

    assert!(
        document
            .html
            .contains("src=\"https://example.test/logo.png\"")
    );
    assert_eq!(document.omitted_images, 0);
    Ok(())
}

#[test]
fn clipboard_document_should_replace_missing_managed_images_with_alt_text()
-> Result<(), Box<dyn std::error::Error>> {
    let document = clipboard_document("![Diagram](assets/missing.png)", None)?;

    assert!(document.html.contains("[Image: Diagram]"));
    assert_eq!(document.omitted_images, 1);
    Ok(())
}

#[test]
fn clipboard_document_should_replace_invalid_managed_asset_paths_with_alt_text()
-> Result<(), Box<dyn std::error::Error>> {
    let document = clipboard_document("![Private](assets/../library.sqlite3)", None)?;

    assert!(document.html.contains("[Image: Private]"));
    assert_eq!(document.omitted_images, 1);
    Ok(())
}

#[test]
fn clipboard_document_should_omit_managed_images_over_the_per_image_limit()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("large.png"),
        vec![0_u8; MAX_EMBEDDED_IMAGE_BYTES + 1],
    )?;

    let document = clipboard_document("![Large](assets/large.png)", Some(directory.path()))?;

    assert!(document.html.contains("[Image: Large]"));
    assert_eq!(document.omitted_images, 1);
    Ok(())
}

#[test]
fn clipboard_document_should_limit_total_embedded_image_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let image = vec![0_u8; MAX_EMBEDDED_IMAGE_BYTES];
    for index in 1..=4 {
        fs::write(directory.path().join(format!("{index}.png")), &image)?;
    }

    let document = clipboard_document(
        "![One](assets/1.png) ![Two](assets/2.png) ![Three](assets/3.png) ![Four](assets/4.png)",
        Some(directory.path()),
    )?;

    assert_eq!(document.html.matches("data:image/png;base64,").count(), 3);
    assert!(document.html.contains("[Image: Four]"));
    assert_eq!(document.omitted_images, 1);
    Ok(())
}

#[test]
fn clipboard_images_should_escape_decoded_alt_text_when_an_asset_is_missing()
-> Result<(), Box<dyn std::error::Error>> {
    let (html, omitted) = embed_managed_images(
        "<img src='assets/missing.png' alt='&lt;script&gt; &amp; A > B'>",
        None,
    )?;
    assert_eq!(html, "<span>[Image: &lt;script&gt; &amp; A &gt; B]</span>");
    assert_eq!(omitted, 1);
    Ok(())
}

#[test]
fn clipboard_images_should_embed_entity_encoded_managed_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("example.png"), [1_u8, 2, 3])?;
    let (html, omitted) = embed_managed_images(
        "<IMG alt='a > b' src='assets&#47;example.png'>",
        Some(directory.path()),
    )?;
    assert!(html.contains("src=\"data:image/png;base64,AQID\""));
    assert_eq!(omitted, 0);
    Ok(())
}

#[test]
fn clipboard_images_should_reject_entity_encoded_traversal()
-> Result<(), Box<dyn std::error::Error>> {
    let (html, omitted) = embed_managed_images("<img src='assets/&#46;&#46;/private.png'>", None)?;
    assert_eq!(html, "<span>[Image omitted]</span>");
    assert_eq!(omitted, 1);
    Ok(())
}
