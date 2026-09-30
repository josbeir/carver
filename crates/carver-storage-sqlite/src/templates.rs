//! Persistence for reusable, revision-checked templates.
use super::{
    CategoryAppearance, CategoryId, NoteTemplate, OffsetDateTime, OptionalExtension, Revision,
    SqliteLibrary, StorageError, TemplateId, Uuid, category_color_value, category_icon_value,
    category_name, params, parse_timestamp, rebuild_category_fts, timestamp, to_sql_error,
};

impl SqliteLibrary {
    /// Updates all category form fields in one transaction.
    ///
    /// # Errors
    /// Returns validation, unavailable-target, or database errors.
    pub fn update_template_category(
        &self,
        id: CategoryId,
        name: &str,
        appearance: CategoryAppearance,
        template: Option<TemplateId>,
        now: OffsetDateTime,
    ) -> Result<(), StorageError> {
        let name = category_name(name)?;
        let transaction = self.connection.unchecked_transaction()?;
        let affected = transaction.execute("UPDATE categories SET name = ?1, icon = ?2, color = ?3, default_template_id = ?4, updated_at = ?5 WHERE id = ?6 AND trashed_at IS NULL AND (?4 IS NULL OR EXISTS(SELECT 1 FROM templates WHERE id = ?4))", params![name, category_icon_value(appearance.icon), category_color_value(appearance.color), template.map(|id| id.to_string()), timestamp(now), id.to_string()])?;
        if affected != 1 {
            return Err(StorageError::TemplateConflict);
        }
        rebuild_category_fts(&transaction, id)?;
        transaction.commit()?;
        Ok(())
    }

    /// Lists all templates in name order.
    ///
    /// # Errors
    /// Returns database or corrupt-value errors.
    pub fn list_templates(&self) -> Result<Vec<NoteTemplate>, StorageError> {
        self.connection.prepare("SELECT id, name, source, revision, created_at, updated_at FROM templates ORDER BY name COLLATE NOCASE, id")?
            .query_map([], template_from_row)?.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    /// Reads one template.
    ///
    /// # Errors
    /// Returns database or corrupt-value errors.
    pub fn get_template(&self, id: TemplateId) -> Result<Option<NoteTemplate>, StorageError> {
        self.connection.query_row("SELECT id, name, source, revision, created_at, updated_at FROM templates WHERE id = ?1", [id.to_string()], template_from_row).optional().map_err(Into::into)
    }
    /// Creates a self-contained canonical source template.
    ///
    /// # Errors
    /// Returns validation or database errors.
    pub fn insert_template(
        &self,
        name: &str,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<NoteTemplate, StorageError> {
        let name = validate_name(name)?;
        carver_domain::validate_template_source(source)?;
        let id = TemplateId::new();
        self.connection.execute("INSERT INTO templates (id, name, source, revision, created_at, updated_at) VALUES (?1, ?2, ?3, 1, ?4, ?4)", params![id.to_string(), name, source, timestamp(now)])?;
        self.get_template(id)?.ok_or(StorageError::TemplateConflict)
    }
    /// Saves a template with optimistic concurrency, preserving no-op timestamps.
    ///
    /// # Errors
    /// Returns validation, conflict, or database errors.
    pub fn update_template(
        &self,
        id: TemplateId,
        revision: Revision,
        name: &str,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<NoteTemplate, StorageError> {
        let name = validate_name(name)?;
        carver_domain::validate_template_source(source)?;
        let transaction = self.connection.unchecked_transaction()?;
        let existing = self
            .get_template(id)?
            .ok_or(StorageError::TemplateConflict)?;
        if existing.revision != revision {
            return Err(StorageError::TemplateConflict);
        }
        if existing.name != name || existing.source != source {
            let changed = transaction.execute("UPDATE templates SET name = ?1, source = ?2, revision = revision + 1, updated_at = ?3 WHERE id = ?4 AND revision = ?5", params![name, source, timestamp(now), id.to_string(), revision.0])?;
            if changed != 1 {
                return Err(StorageError::TemplateConflict);
            }
        }
        transaction.commit()?;
        self.get_template(id)?.ok_or(StorageError::TemplateConflict)
    }
    /// Deletes a template and atomically clears category references.
    ///
    /// # Errors
    /// Returns a conflict for stale revisions or missing templates, or database errors.
    pub fn remove_template(&self, id: TemplateId, revision: Revision) -> Result<(), StorageError> {
        let affected = self.connection.execute(
            "DELETE FROM templates WHERE id = ?1 AND revision = ?2",
            params![id.to_string(), revision.0],
        )?;
        if affected != 1 {
            return Err(StorageError::TemplateConflict);
        }
        Ok(())
    }
    /// Assigns a template to an active category, or clears its assignment.
    ///
    /// # Errors
    /// Returns an error if the category/template is unavailable or the database fails.
    pub fn assign_category_template(
        &self,
        id: CategoryId,
        template: Option<TemplateId>,
        now: OffsetDateTime,
    ) -> Result<(), StorageError> {
        let affected = self.connection.execute("UPDATE categories SET default_template_id = ?1, updated_at = CASE WHEN default_template_id IS ?1 THEN updated_at ELSE ?2 END WHERE id = ?3 AND trashed_at IS NULL AND (?1 IS NULL OR EXISTS(SELECT 1 FROM templates WHERE id = ?1))", params![template.map(|id| id.to_string()), timestamp(now), id.to_string()])?;
        if affected != 1 {
            return Err(StorageError::TemplateConflict);
        }
        Ok(())
    }
}
fn validate_name(name: &str) -> Result<&str, StorageError> {
    let name = name.trim();
    if name.is_empty() {
        Err(StorageError::InvalidTemplateName)
    } else {
        Ok(name)
    }
}
fn template_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteTemplate> {
    Ok(NoteTemplate {
        id: TemplateId::from_uuid(
            Uuid::parse_str(&row.get::<_, String>(0)?)
                .map_err(|e| to_sql_error(StorageError::Corrupt(e.to_string())))?,
        ),
        name: row.get(1)?,
        source: row.get(2)?,
        revision: Revision(row.get(3)?),
        created_at: parse_timestamp(row.get(4)?).map_err(to_sql_error)?,
        updated_at: parse_timestamp(row.get(5)?).map_err(to_sql_error)?,
    })
}
