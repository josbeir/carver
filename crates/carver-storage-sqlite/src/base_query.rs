use carver_domain::{
    BaseColumn, BaseFilter, BaseFilterMode, BaseFilterOperator, BaseSort, BaseSortDirection,
    PropertyPath,
};
use rusqlite::types::Value as SqlValue;
use serde_json::Value;

pub(super) struct BaseQuery {
    pub(super) filter_sql: Option<String>,
    pub(super) order_sql: String,
    pub(super) parameters: Vec<SqlValue>,
}

pub(super) fn compile_base_query(
    mode: BaseFilterMode,
    filters: &[BaseFilter],
    sorts: &[BaseSort],
) -> BaseQuery {
    let mut parameters = Vec::new();
    let filter_sql = compile_filter_sql(mode, filters, &mut parameters);
    let order_sql = compile_order_sql(sorts, &mut parameters);
    BaseQuery {
        filter_sql,
        order_sql,
        parameters,
    }
}

pub(super) fn compile_base_filter(
    mode: BaseFilterMode,
    filters: &[BaseFilter],
) -> (Option<String>, Vec<SqlValue>) {
    let mut parameters = Vec::new();
    let sql = compile_filter_sql(mode, filters, &mut parameters);
    (sql, parameters)
}

fn compile_filter_sql(
    mode: BaseFilterMode,
    filters: &[BaseFilter],
    parameters: &mut Vec<SqlValue>,
) -> Option<String> {
    if filters.is_empty() {
        return None;
    }
    let separator = match mode {
        BaseFilterMode::All => " AND ",
        BaseFilterMode::Any => " OR ",
    };
    let filters = filters
        .iter()
        .map(|filter| compile_filter(filter, parameters))
        .collect::<Vec<_>>();
    Some(format!("({})", filters.join(separator)))
}

fn compile_filter(filter: &BaseFilter, parameters: &mut Vec<SqlValue>) -> String {
    match filter.operator {
        BaseFilterOperator::IsPresent => field_is_present(&filter.field, parameters),
        BaseFilterOperator::IsMissing => {
            format!("NOT ({})", field_is_present(&filter.field, parameters))
        }
        BaseFilterOperator::Contains => text_match(filter, parameters, false),
        BaseFilterOperator::StartsWith => text_match(filter, parameters, true),
        BaseFilterOperator::GreaterThan
        | BaseFilterOperator::LessThan
        | BaseFilterOperator::GreaterOrEqual
        | BaseFilterOperator::LessOrEqual => numeric_match(filter, parameters),
        BaseFilterOperator::Equals => scalar_equal(filter, parameters),
        BaseFilterOperator::NotEquals => {
            let present = field_is_present(&filter.field, parameters);
            let equal = scalar_equal(filter, parameters);
            format!("({present} AND NOT ({equal}))")
        }
        BaseFilterOperator::ListContains => list_match(filter, parameters, false),
        BaseFilterOperator::ListNotContains => list_match(filter, parameters, true),
    }
}

fn text_match(filter: &BaseFilter, parameters: &mut Vec<SqlValue>, starts_with: bool) -> String {
    let Some(Value::String(expected)) = filter.value.as_ref() else {
        return "0".to_owned();
    };
    let value_type = field_type(&filter.field, parameters);
    let value = field_value(&filter.field, parameters);
    parameters.push(SqlValue::Text(expected.clone()));
    if starts_with {
        parameters.push(SqlValue::Text(expected.clone()));
        format!(
            "({value_type} = 'text' AND substr(carver_casefold({value}), 1, length(carver_casefold(?))) = carver_casefold(?))"
        )
    } else {
        format!(
            "({value_type} = 'text' AND instr(carver_casefold({value}), carver_casefold(?)) > 0)"
        )
    }
}

fn numeric_match(filter: &BaseFilter, parameters: &mut Vec<SqlValue>) -> String {
    let Some(expected) = filter.value.as_ref().and_then(Value::as_f64) else {
        return "0".to_owned();
    };
    let value_type = field_type(&filter.field, parameters);
    let value = field_value(&filter.field, parameters);
    parameters.push(SqlValue::Real(expected));
    let operator = match filter.operator {
        BaseFilterOperator::GreaterThan => ">",
        BaseFilterOperator::LessThan => "<",
        BaseFilterOperator::GreaterOrEqual => ">=",
        BaseFilterOperator::LessOrEqual => "<=",
        _ => return "0".to_owned(),
    };
    format!("({value_type} IN ('integer', 'real') AND {value} {operator} ?)")
}

fn scalar_equal(filter: &BaseFilter, parameters: &mut Vec<SqlValue>) -> String {
    let Some(expected) = filter.value.as_ref() else {
        return "0".to_owned();
    };
    match expected {
        Value::String(expected) => {
            let value_type = field_type(&filter.field, parameters);
            let value = field_value(&filter.field, parameters);
            parameters.push(SqlValue::Text(expected.clone()));
            format!("({value_type} = 'text' AND carver_casefold({value}) = carver_casefold(?))")
        }
        Value::Number(expected) => {
            let Some(expected) = expected.as_f64() else {
                return "0".to_owned();
            };
            let value_type = field_type(&filter.field, parameters);
            let value = field_value(&filter.field, parameters);
            parameters.push(SqlValue::Real(expected));
            format!("({value_type} IN ('integer', 'real') AND {value} = ?)")
        }
        Value::Bool(expected) => {
            let value_type = field_type(&filter.field, parameters);
            let expected_type = if *expected { "true" } else { "false" };
            format!("({value_type} = '{expected_type}')")
        }
        Value::Null | Value::Array(_) | Value::Object(_) => "0".to_owned(),
    }
}

fn list_match(filter: &BaseFilter, parameters: &mut Vec<SqlValue>, negated: bool) -> String {
    let value_type = field_type(&filter.field, parameters);
    let Some(path) = property_path(&filter.field) else {
        return "0".to_owned();
    };
    parameters.push(SqlValue::Text(path));
    let entry_matches = list_entry_match(filter.value.as_ref(), parameters);
    let exists = format!(
        "EXISTS (SELECT 1 FROM json_each(n.frontmatter_json, ?) AS entry WHERE {entry_matches})"
    );
    let not = if negated { "NOT " } else { "" };
    format!("({value_type} = 'array' AND {not}({exists}))")
}

fn list_entry_match(value: Option<&Value>, parameters: &mut Vec<SqlValue>) -> String {
    match value {
        Some(Value::String(expected)) => {
            parameters.push(SqlValue::Text(expected.clone()));
            "(entry.type = 'text' AND carver_casefold(entry.value) = carver_casefold(?))".to_owned()
        }
        Some(Value::Number(expected)) => {
            let Some(expected) = expected.as_f64() else {
                return "0".to_owned();
            };
            parameters.push(SqlValue::Real(expected));
            "(entry.type IN ('integer', 'real') AND entry.value = ?)".to_owned()
        }
        Some(Value::Bool(expected)) => {
            let entry_type = if *expected { "true" } else { "false" };
            format!("entry.type = '{entry_type}'")
        }
        Some(Value::Null | Value::Array(_) | Value::Object(_)) | None => "0".to_owned(),
    }
}

fn compile_order_sql(sorts: &[BaseSort], parameters: &mut Vec<SqlValue>) -> String {
    if sorts.is_empty() {
        return "n.updated_at DESC, n.id ASC".to_owned();
    }
    let mut terms = Vec::new();
    for sort in sorts {
        let direction = match sort.direction {
            BaseSortDirection::Ascending => "ASC",
            BaseSortDirection::Descending => "DESC",
        };
        let present = field_is_present(&sort.field, parameters);
        terms.push(format!("CASE WHEN {present} THEN 0 ELSE 1 END ASC"));

        let value_type = field_type(&sort.field, parameters);
        terms.push(format!(
            "CASE {value_type} WHEN 'false' THEN 1 WHEN 'true' THEN 1 WHEN 'integer' THEN 2 WHEN 'real' THEN 2 WHEN 'text' THEN 3 WHEN 'array' THEN 4 WHEN 'object' THEN 5 ELSE 0 END ASC"
        ));

        let text_type = field_type(&sort.field, parameters);
        let text_value = field_value(&sort.field, parameters);
        terms.push(format!(
            "CASE WHEN {text_type} = 'text' THEN carver_casefold({text_value}) END {direction}"
        ));

        let number_type = field_type(&sort.field, parameters);
        let number_value = field_value(&sort.field, parameters);
        terms.push(format!(
            "CASE WHEN {number_type} IN ('integer', 'real') THEN CAST({number_value} AS REAL) END {direction}"
        ));

        let bool_type = field_type(&sort.field, parameters);
        let bool_value = field_value(&sort.field, parameters);
        terms.push(format!(
            "CASE WHEN {bool_type} IN ('false', 'true') THEN {bool_value} END {direction}"
        ));

        let complex_type = field_type(&sort.field, parameters);
        let complex_value = field_value(&sort.field, parameters);
        terms.push(format!(
            "CASE WHEN {complex_type} IN ('array', 'object') THEN CAST({complex_value} AS TEXT) END {direction}"
        ));
    }
    terms.push("n.id ASC".to_owned());
    terms.join(", ")
}

fn field_is_present(field: &BaseColumn, parameters: &mut Vec<SqlValue>) -> String {
    match field {
        BaseColumn::Property(_) => {
            let value_type = field_type(field, parameters);
            format!("COALESCE({value_type}, 'null') <> 'null'")
        }
        BaseColumn::Name | BaseColumn::Category | BaseColumn::Updated => "1".to_owned(),
    }
}

fn field_type(field: &BaseColumn, parameters: &mut Vec<SqlValue>) -> String {
    match field {
        BaseColumn::Name | BaseColumn::Category | BaseColumn::Updated => "'text'".to_owned(),
        BaseColumn::Property(_) => {
            let Some(path) = property_path(field) else {
                return "NULL".to_owned();
            };
            parameters.push(SqlValue::Text(path));
            "json_type(n.frontmatter_json, ?)".to_owned()
        }
    }
}

fn field_value(field: &BaseColumn, parameters: &mut Vec<SqlValue>) -> String {
    match field {
        BaseColumn::Name => "n.title".to_owned(),
        BaseColumn::Category => "c.name".to_owned(),
        BaseColumn::Updated => {
            "strftime('%Y-%m-%dT%H:%M:%SZ', n.updated_at, 'unixepoch')".to_owned()
        }
        BaseColumn::Property(_) => {
            let Some(path) = property_path(field) else {
                return "NULL".to_owned();
            };
            parameters.push(SqlValue::Text(path));
            "json_extract(n.frontmatter_json, ?)".to_owned()
        }
    }
}

fn property_path(field: &BaseColumn) -> Option<String> {
    let BaseColumn::Property(path) = field else {
        return None;
    };
    sqlite_json_path(path)
}

fn sqlite_json_path(pointer: &PropertyPath) -> Option<String> {
    let mut path = "$".to_owned();
    let raw = pointer.0.strip_prefix('/')?;
    for segment in raw.split('/') {
        let segment = unescape_pointer_segment(segment)?;
        if let Ok(index) = segment.parse::<usize>() {
            path.push('[');
            path.push_str(&index.to_string());
            path.push(']');
        } else {
            path.push('.');
            path.push_str(&serde_json::to_string(&segment).ok()?);
        }
    }
    Some(path)
}

fn unescape_pointer_segment(segment: &str) -> Option<String> {
    let mut output = String::with_capacity(segment.len());
    let mut characters = segment.chars();
    while let Some(character) = characters.next() {
        if character != '~' {
            output.push(character);
            continue;
        }
        match characters.next()? {
            '0' => output.push('~'),
            '1' => output.push('/'),
            _ => return None,
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_pointer_should_escape_object_labels_for_json1() {
        assert_eq!(
            sqlite_json_path(&PropertyPath("/project~1status/a.b/~0name".to_owned())),
            Some("$.\"project/status\".\"a.b\".\"~name\"".to_owned())
        );
    }
}
