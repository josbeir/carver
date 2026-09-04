//! External-consumer contract tests for `carver-sdk`.

use std::{error::Error, fmt};

use carver_sdk::LibraryError;

#[derive(Debug)]
struct TestError;

impl fmt::Display for TestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("test backend failure")
    }
}

impl Error for TestError {}

#[test]
fn library_error_should_expose_worker_unavailability_to_external_clients() {
    let error = LibraryError::<TestError>::Unavailable;

    assert_eq!(error.to_string(), "library storage worker is unavailable");
}
