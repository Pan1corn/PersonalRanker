use rusqlite::{params, OptionalExtension};

use crate::{
    domain::media::MediaAssetRecord, error::AppError,
    repository::project_repository::ProjectRepository,
};

impl ProjectRepository {
    pub(crate) fn save_media_assets(
        &mut self,
        assets: &[MediaAssetRecord],
    ) -> Result<(), AppError> {
        let transaction = self.connection_mut().transaction()?;
        for asset in assets {
            transaction.execute(
                "INSERT INTO media_assets (id, dataset_id, item_id, field_name, source_kind, source_value, local_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![asset.id, asset.dataset_id, asset.item_id, asset.field_name, asset.source_kind, asset.source_value, asset.local_path, asset.created_at],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn replace_media_assets(
        &mut self,
        assets: &[MediaAssetRecord],
    ) -> Result<Vec<String>, AppError> {
        let transaction = self.connection_mut().transaction()?;
        let mut replaced_paths = Vec::new();
        for asset in assets {
            if let Some(old_path) = transaction
                .query_row(
                    "SELECT local_path FROM media_assets WHERE item_id = ?1 AND field_name = ?2",
                    params![asset.item_id, asset.field_name],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten()
            {
                if asset.local_path.as_deref() != Some(old_path.as_str()) {
                    replaced_paths.push(old_path);
                }
            }
            transaction.execute(
                "INSERT INTO media_assets (id, dataset_id, item_id, field_name, source_kind, source_value, local_path, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT(item_id, field_name) DO UPDATE SET \
                    id = excluded.id, dataset_id = excluded.dataset_id, source_kind = excluded.source_kind, \
                    source_value = excluded.source_value, local_path = excluded.local_path, created_at = excluded.created_at",
                params![asset.id, asset.dataset_id, asset.item_id, asset.field_name, asset.source_kind, asset.source_value, asset.local_path, asset.created_at],
            )?;
        }
        transaction.commit()?;
        Ok(replaced_paths)
    }

    pub(crate) fn media_asset(
        &self,
        item_id: &str,
        field_name: &str,
    ) -> Result<Option<MediaAssetRecord>, AppError> {
        self.connection()
            .query_row(
                "SELECT id, dataset_id, item_id, field_name, source_kind, source_value, local_path, created_at FROM media_assets WHERE item_id = ?1 AND field_name = ?2",
                params![item_id, field_name],
                |row| {
                    Ok(MediaAssetRecord {
                        id: row.get(0)?,
                        dataset_id: row.get(1)?,
                        item_id: row.get(2)?,
                        field_name: row.get(3)?,
                        source_kind: row.get(4)?,
                        source_value: row.get(5)?,
                        local_path: row.get(6)?,
                        created_at: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }
}
