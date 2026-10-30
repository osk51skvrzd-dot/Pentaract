
use base64::{engine::general_purpose::STANDARD, Engine};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::{PentaractError, PentaractResult},
    models::files::{DBFSElement, FSElement},
};

pub struct FilesRepository<'d> {
    db: &'d PgPool,
}

impl<'d> FilesRepository<'d> {
    pub fn new(db: &'d PgPool) -> Self {
        Self { db }
    }

    pub async fn list_dir(
        &self,
        storage_id: Uuid,
        path: &str,
    ) -> PentaractResult<Vec<FSElement>> {
        let files = sqlx::query_as!(
            DBFSElement,
            r#"
            SELECT
                id,
                name,
                path,
                is_dir,
                size,
                uploaded,
                thumbnail
            FROM files
            WHERE storage_id = $1
              AND path = $2
            ORDER BY is_dir DESC, name ASC
            "#,
            storage_id,
            path
        )
        .fetch_all(self.db)
        .await?;

        Ok(files
            .into_iter()
            .map(|file| {
                let thumbnail = file.thumbnail.map(|data| {
                    format!("data:image/webp;base64,{}", STANDARD.encode(data))
                });

                FSElement {
                    id: file.id,
                    name: file.name,
                    path: file.path,
                    is_dir: file.is_dir,
                    size: file.size,
                    uploaded: file.uploaded,
                    thumbnail,
                }
            })
            .collect())
    }

    pub async fn set_thumbnail(
        &self,
        file_id: Uuid,
        thumbnail: &[u8],
    ) -> PentaractResult<()> {
        sqlx::query!(
            r#"
            UPDATE files
            SET thumbnail = $1
            WHERE id = $2
            "#,
            thumbnail,
            file_id
        )
        .execute(self.db)
        .await?;

        Ok(())
    }

    // Остальные методы репозитория должны оставаться
    // такими же, как в вашей исходной версии.
}
