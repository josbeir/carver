use super::*;
use crate::mvu::{TemplatePurpose, TemplatesMsg};

#[test]
fn template_load_should_ignore_stale_completions() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Manage));
    let previous = model.template_request.unwrap_or(RequestId(0));
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Manage));
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::Loaded {
                request_id: previous,
                purpose: TemplatePurpose::Manage,
                result: Ok(Vec::new())
            })
        )
        .is_empty()
    );
}
#[test]
fn template_save_should_reject_invalid_properties_before_persistence() {
    let mut config = Config::default();
    config.document_properties.entries = vec![carver_config::DocumentProperty {
        key: "count".into(),
        field_type: carver_config::DocumentPropertyType::Number,
        multiple: false,
        value: serde_json::json!(1),
    }];
    let mut model = AppModel::new(&config);
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Edit(None)));
    let id = model.template_editor.unwrap_or(RequestId(0));
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Save {
            request_id: id,
            original: None,
            name: "Template".into(),
            source: "---\ncount: text\n---".into(),
        }),
    );
    assert!(
        matches!(&effects[..], [Effect::FinishTemplateEdit { error: Some(error), .. }] if error.message.contains("count"))
    );
}
#[test]
fn template_save_should_accept_explicit_empty_values() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Edit(None)));
    let id = model.template_editor.unwrap_or(RequestId(0));
    let source = "---\ntext: ''\nlist: []\nflag: false\nnumber: 0\nempty: null\n---";
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Save {
            request_id: id,
            original: None,
            name: " Template ".into(),
            source: source.into(),
        }),
    );
    assert!(
        matches!(&effects[..], [Effect::SaveTemplate { name, source: saved, .. }] if name == "Template" && saved == source)
    );
}
#[test]
fn template_save_should_ignore_a_closed_editor() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Edit(None)));
    let id = model.template_editor.unwrap_or(RequestId(0));
    let _ = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::EditorClosed(id)),
    );
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::Saved {
                request_id: id,
                result: Ok(())
            })
        )
        .is_empty()
    );
}
#[test]
fn ordinary_note_creation_should_resolve_category_template_in_the_runtime() {
    let mut model = AppModel::new(&Config::default());
    let id = CategoryId::new();
    model.selected_category = Some(id);
    assert_eq!(
        update(&mut model, AppMsg::Navigation(NavigationMsg::CreateNote)),
        vec![Effect::CreateCategoryNote { category_id: id }]
    );
}

#[test]
fn new_category_should_load_choices_for_the_shared_creation_form() {
    let mut model = AppModel::new(&Config::default());
    let effects = update(&mut model, AppMsg::Templates(TemplatesMsg::NewCategory));
    assert!(matches!(
        &effects[..],
        [Effect::LoadTemplates {
            purpose: TemplatePurpose::NewCategory,
            ..
        }]
    ));
}

#[test]
fn category_creation_should_forward_the_selected_template() {
    let mut model = AppModel::new(&Config::default());
    let template_id = carver_sdk::TemplateId::new();
    let effects = update(
        &mut model,
        AppMsg::Action(ActionMsg::CreateCategoryWithAppearance {
            name: " Work ".into(),
            appearance: carver_sdk::CategoryAppearance::default(),
            template_id: Some(template_id),
        }),
    );
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::CreateCategoryWithAppearance { name, template_id: Some(id), .. } if name == "Work" && *id == template_id)));
}
