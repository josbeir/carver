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
fn batch_staging_should_finish_before_any_file_is_stored() -> TestResult {
    let directory = tempfile::tempdir()?;
    let valid = directory.path().join("valid.txt");
    std::fs::write(&valid, "valid")?;
    let oversized = directory.path().join("oversized.bin");
    std::fs::File::create(&oversized)?.set_len(MAX_FILE_BYTES as u64 + 1)?;
    let files = vec![
        ImportFileSource {
            uri: gio::File::for_path(valid).uri().to_string(),
            label: "valid.txt".into(),
            extension: "txt".into(),
            image: false,
        },
        ImportFileSource {
            uri: gio::File::for_path(oversized).uri().to_string(),
            label: "oversized.bin".into(),
            extension: "bin".into(),
            image: false,
        },
    ];
    let context = glib::MainContext::new();
    context.with_thread_default(|| {
        assert!(context.block_on(stage_native_files(files)).is_err());
    })?;
    Ok(())
}

#[test]
fn batch_staging_should_enforce_the_total_file_limit() -> TestResult {
    assert_eq!(
        remaining_batch_capacity(MAX_BATCH_BYTES - MAX_FILE_BYTES, MAX_BATCH_BYTES),
        Ok(MAX_FILE_BYTES)
    );
    assert!(remaining_batch_capacity(MAX_BATCH_BYTES, MAX_BATCH_BYTES).is_err());

    let directory = tempfile::tempdir()?;
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    std::fs::write(&first, "12")?;
    std::fs::write(&second, "34")?;
    let files = [first, second]
        .into_iter()
        .map(|path| ImportFileSource {
            uri: gio::File::for_path(path).uri().to_string(),
            label: "file.txt".into(),
            extension: "txt".into(),
            image: false,
        })
        .collect();
    let context = glib::MainContext::new();
    context.with_thread_default(|| {
        assert!(
            context
                .block_on(stage_native_files_with_limit(files, 3))
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
