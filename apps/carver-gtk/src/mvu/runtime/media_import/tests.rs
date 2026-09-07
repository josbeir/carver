use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn file_read_should_reject_oversized_files_before_loading_their_contents() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("large.bin");
    std::fs::File::create(&path)?.set_len(MAX_FILE_BYTES as u64 + 1)?;
    let context = glib::MainContext::new();
    context.with_thread_default(|| {
        assert!(
            context
                .block_on(read_bounded_file(&gio::File::for_path(path)))
                .is_err()
        );
        assert!(
            context
                .block_on(read_bounded_file(&gio::File::for_path(directory.path())))
                .is_err()
        );
    })?;
    Ok(())
}

#[test]
fn stream_read_should_enforce_the_limit_even_without_file_metadata() -> TestResult {
    let context = glib::MainContext::new();
    context.with_thread_default(|| {
        let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(b"12345"));
        assert!(context.block_on(read_bounded_stream(&stream, 4)).is_err());
        let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(b"1234"));
        assert_eq!(
            context.block_on(read_bounded_stream(&stream, 4)),
            Ok(b"1234".to_vec())
        );
    })?;
    Ok(())
}

#[test]
fn thumbnail_should_bound_dimensions_and_preserve_aspect_ratio() -> TestResult {
    use gtk::gdk_pixbuf::prelude::*;
    let image = gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 1200, 600)
        .ok_or("image fixture")?;
    image.fill(0x33aa_66ff);
    let bytes = image.save_to_bufferv("png", &[])?;
    let bytes = thumbnail(&bytes).ok_or("thumbnail")?;
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    loader.write(&bytes)?;
    loader.close()?;
    let scaled = loader.pixbuf().ok_or("decoded thumbnail")?;
    assert_eq!((scaled.width(), scaled.height()), (96, 48));
    assert!(thumbnail(b"not an image").is_none());
    Ok(())
}
