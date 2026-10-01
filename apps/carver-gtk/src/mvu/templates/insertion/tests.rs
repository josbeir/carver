use super::*;

#[test]
fn insertion_should_add_missing_properties_without_overwriting_existing_values()
-> Result<(), UiError> {
    let config = DocumentPropertiesConfig::default();
    let (body, properties) = prepare(
        "---\nstatus: draft\nkind: meeting\n---\n# Agenda\n",
        "---\nstatus: done\n---\nExisting\n",
        &config,
    )?;
    assert!(body.contains("Agenda"));
    let Some(properties) = properties else {
        return Err(UiError::new("missing properties"));
    };
    assert_eq!(properties.fields.len(), 2);
    assert_eq!(
        properties.fields[0].value,
        carver_domain::FrontmatterValue::Text("done".into())
    );
    assert_eq!(properties.fields[1].key, "kind");
    Ok(())
}

#[test]
fn insertion_should_preserve_metadata_when_all_template_properties_already_exist()
-> Result<(), UiError> {
    let (_, properties) = prepare(
        "---\nstatus: draft\n---\n",
        "---\nstatus:\n---\n",
        &DocumentPropertiesConfig::default(),
    )?;
    assert!(properties.is_none());
    Ok(())
}

#[test]
fn insertion_should_reject_invalid_existing_metadata_before_adding_properties() {
    assert!(
        prepare(
            "---\nkind: meeting\n---\n",
            "---\nbroken: [\n---\n",
            &DocumentPropertiesConfig::default()
        )
        .is_err()
    );
}

#[test]
fn insertion_patterns_should_resolve_without_overwriting_existing_properties() -> Result<(), UiError>
{
    let sample = carver_domain::TemplateContext {
        now: time::macros::datetime!(2026-10-01 14:30 +02:00),
        category: "Meetings".into(),
    };
    let (body, properties) = prepare_at(
        "---\nexisting: '{{date}}'\nadded: '{{datetime}}'\n---\n# {{category}}",
        "---\nexisting: original\n---\n",
        &DocumentPropertiesConfig::default(),
        &sample,
    )?;
    assert_eq!(body, "# Meetings");
    let properties = properties.ok_or_else(|| UiError::new("missing properties"))?;
    assert_eq!(
        properties.fields[0].value,
        carver_domain::FrontmatterValue::Text("original".into())
    );
    assert_eq!(
        properties.fields[1].value,
        carver_domain::FrontmatterValue::Text("2026-10-01T14:30:00+02:00".into())
    );
    Ok(())
}
