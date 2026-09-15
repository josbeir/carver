use super::*;

#[test]
fn migrations_should_be_valid() {
    migrations()
        .validate()
        .unwrap_or_else(|error| panic!("migration definition is invalid: {error}"));
}
