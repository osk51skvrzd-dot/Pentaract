
use axum::body::Bytes;
use sqlx::PgPool;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::{
    common::{
        access::check_access,
        channels::{
            ClientData, ClientMessage, ClientSender, DownloadFileData, StorageManagerData,
            UploadFileData,
        },
        jwt_manager::AuthUser,
    },
    errors::{PentaractError, PentaractResult},
    models::{
        access::AccessType,
        files::{FSElement, File, InFile, SearchFSElement},
    },
    repositories::{
        access::AccessRepository, files::FilesRepository,
        storage_workers::StorageWorkersRepository,
    },
    schemas::files::{InFileSchema, InFolderSchema},
};

pub struct FilesService<'d> {
    repo: FilesRepository<'d>,
    storage_workers_repo: StorageWorkersRepository<'d>,
    access_repo: AccessRepository<'d>,
    tx: ClientSender,
}

impl<'d> FilesService<'d> {
    pub fn new(db: &'d PgPool, tx: ClientSender) -> Self {
        Self {
            repo: FilesRepository::new(db),
            storage_workers_repo: StorageWorkersRepository::new(db),
            access_repo: AccessRepository::new(db),
            tx,
        }
    }

    pub async fn create_folder(
        &self,
        in_schema: InFolderSchema,
        user: &AuthUser,
    ) -> PentaractResult<()> {
        check_access(
            &self.access_repo,
            user.id,
            in_schema.storage_id,
            &AccessType::W,
        )
        .await?;

        if !Self::validate_filepath(&in_schema.parent_path) {
            return Err(PentaractError::InvalidPath);
        }

        if in_schema.folder_name.contains('/') {
            return Err(PentaractError::InvalidFolderName);
        }

        let path = if !in_schema.parent_path.is_empty() {
            format!("{}/{}/", in_schema.parent_path, in_schema.folder_name)
        } else {
            format!("{}/", in_schema.folder_name)
        };

        let in_file = InFile::new(path, 0, in_schema.storage_id);

        self.repo.create_folder(in_file).await.map(|_| ())
    }

    pub async fn upload_to(
        &self,
        in_schema: InFileSchema,
        user: &AuthUser,
    ) -> PentaractResult<()> {
        check_access(
            &self.access_repo,
            user.id,
            in_schema.storage_id,
            &AccessType::W,
        )
        .await?;

        self.check_storage_workers(in_schema.storage_id).await?;

        if !Self::validate_filepath(&in_schema.path) {
            return Err(PentaractError::InvalidPath);
        }

        let in_file = InFile::new(
            in_schema.path,
            in_schema.size,
            in_schema.storage_id,
        );

        let file = self.repo.create_file(in_file).await?;

        self._upload(file, in_schema.file, user).await
    }

    pub async fn upload_anyway(
        &self,
        in_file: InFile,
        file_data: Bytes,
        user: &AuthUser,
    ) -> PentaractResult<()> {
        check_access(
            &self.access_repo,
            user.id,
            in_file.storage_id,
            &AccessType::W,
        )
        .await?;

        self.check_storage_workers(in_file.storage_id).await?;

        let file = self.repo.create_file_anyway(in_file).await?;

        self._upload(file, file_data, user).await
    }

    async fn _upload(
        &self,
        file: File,
        file_data: Bytes,
        user: &AuthUser,
    ) -> PentaractResult<()> {
        // Создаём миниатюру отдельно от загрузки оригинала.
        // Ошибка миниатюры не должна прерывать загрузку файла.
        let thumbnail = Self::generate_thumbnail(file_data.to_vec()).await;

        let (resp_tx, resp_rx) = oneshot::channel();

        let upload_file_data = UploadFileData {
            file_id: file.id,
            user_id: user.id,
            file_data: file_data.as_ref().into(),
        };

        let message = ClientMessage {
            data: ClientData::UploadFile(upload_file_data),
            tx: resp_tx,
        };

        tracing::debug!("sending task to manager");

        let _ = self.tx.send(message).await;

        let message_back = match resp_rx.await.unwrap().data {
            StorageManagerData::UploadFile(r) => r,
            _ => unimplemented!(),
        };

        if let Err(e) = message_back.and({
            tracing::debug!("file loaded successfully");
            self.repo.set_as_uploaded(file.id).await
        }) {
            tracing::error!("{e}");

            let _ = self.repo.delete_with_folders(file.id).await;

            return Err(e);
        }

        // Сохраняем миниатюру после успешной загрузки оригинала.
        if let Some(thumbnail) = thumbnail {
            if let Err(e) = self.repo.set_thumbnail(file.id, &thumbnail).await {
                tracing::warn!("Could not save thumbnail: {e}");
            }
        }

        Ok(())
    }

    // Размер создаваемой миниатюры: 64x64 пикселя.
    async fn generate_thumbnail(data: Vec<u8>) -> Option<Vec<u8>> {
        tokio::task::spawn_blocking(move || {
            let image = image::load_from_memory(&data).ok()?;

            let thumbnail = image.thumbnail(64, 64);

            let encoder = webp::Encoder::from_image(&thumbnail).ok()?;

            Some(encoder.encode(20.0).to_vec())
        })
        .await
        .ok()
        .flatten()
    }

    async fn check_storage_workers(
        &self,
        storage_id: Uuid,
    ) -> PentaractResult<()> {
        if !self.storage_workers_repo.storage_has_any(storage_id).await? {
            Err(PentaractError::StorageDoesNotHaveWorkers)
        } else {
            Ok(())
        }
    }

    pub async fn download(
        &self,
        path: &str,
        storage_id: Uuid,
        user: &AuthUser,
    ) -> PentaractResult<Vec<u8>> {
        check_access(
            &self.access_repo,
            user.id,
            storage_id,
            &AccessType::R,
        )
        .await?;

        if !Self::validate_path(path) {
            return Err(PentaractError::InvalidPath);
        }

        let file = self.repo.get_file_by_path(path, storage_id).await?;

        let (resp_tx, resp_rx) = oneshot::channel();

        let download_file_data = DownloadFileData {
            file_id: file.id,
            storage_id,
            user_id: user.id,
        };

        let message = ClientMessage {
            data: ClientData::DownloadFile(download_file_data),
            tx: resp_tx,
        };

        tracing::debug!("sending task to manager");

        let _ = self.tx.send(message).await;

        match resp_rx.await.unwrap().data {
            StorageManagerData::DownloadFile(r) => r,
            _ => unimplemented!(),
        }
    }

    pub async fn list_dir(
        self,
        storage_id: Uuid,
        path: &str,
        user: &AuthUser,
    ) -> PentaractResult<Vec<FSElement>> {
        check_access(
            &self.access_repo,
            user.id,
            storage_id,
            &AccessType::R,
        )
        .await?;

        self.repo.list_dir(storage_id, path).await
    }

    pub async fn search(
        self,
        storage_id: Uuid,
        path: &str,
        search_path: &str,
        user: &AuthUser,
    ) -> PentaractResult<Vec<SearchFSElement>> {
        check_access(
            &self.access_repo,
            user.id,
            storage_id,
            &AccessType::R,
        )
        .await?;

        self.repo.search(search_path, path, storage_id).await
    }

    pub async fn rename(
        &self,
        old_path: &str,
        new_path: &str,
        storage_id: Uuid,
        user: &AuthUser,
    ) -> PentaractResult<()> {
        check_access(
            &self.access_repo,
            user.id,
            storage_id,
            &AccessType::W,
        )
        .await?;

        if !Self::validate_path(old_path) || !Self::validate_path(new_path) {
            return Err(PentaractError::InvalidPath);
        }

        self.repo.update_path(old_path, new_path, storage_id).await
    }

    pub async fn delete(
        &self,
        path: &str,
        storage_id: Uuid,
        user: &AuthUser,
    ) -> PentaractResult<()> {
        check_access(
            &self.access_repo,
            user.id,
            storage_id,
            &AccessType::W,
        )
        .await?;

        if !Self::validate_path(path) {
            return Err(PentaractError::InvalidPath);
        }

        self.repo.delete(path, storage_id).await
    }

    fn validate_filepath(path: &str) -> bool {
        Self::validate_path(path) && !path.ends_with('/')
    }

    fn validate_path(path: &str) -> bool {
        !path.starts_with('/') && !path.contains("//")
    }
}
