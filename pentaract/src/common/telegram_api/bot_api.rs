
use std::sync::OnceLock;

use reqwest::{multipart, Client};
use uuid::Uuid;

use crate::{
    common::types::ChatId,
    errors::PentaractResult,
    services::storage_workers_scheduler::StorageWorkersScheduler,
};

use super::schemas::{DownloadBodySchema, UploadBodySchema, UploadSchema};

// Один HTTP-клиент на всё приложение.
// reqwest::Client можно безопасно переиспользовать.
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

fn http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(Client::new)
}

pub struct TelegramBotApi<'t> {
    base_url: &'t str,
    scheduler: StorageWorkersScheduler<'t>,
}

impl<'t> TelegramBotApi<'t> {
    pub fn new(
        base_url: &'t str,
        scheduler: StorageWorkersScheduler<'t>,
    ) -> Self {
        Self {
            base_url,
            scheduler,
        }
    }

    pub async fn upload(
        &self,
        file: &[u8],
        chat_id: ChatId,
        storage_id: Uuid,
    ) -> PentaractResult<UploadSchema> {
        let chat_id = {
            // Вставляем 100 между минусом и ID чата.
            let n = chat_id.abs().checked_ilog10().unwrap_or(0) + 1;

            chat_id - (100 * ChatId::from(10).pow(n))
        };

        let token = self.scheduler.get_token(storage_id).await?;

        let url = self.build_url("", "sendDocument", token);

        let file_part = multipart::Part::bytes(file.to_vec())
            .file_name("pentaract_chunk.bin");

        let form = multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .part("document", file_part);

        let response = http_client()
            .post(url)
            .multipart(form)
            .send()
            .await?;

        match response.error_for_status() {
            Ok(response) => {
                Ok(response.json::<UploadBodySchema>().await?.result.document)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn download(
        &self,
        telegram_file_id: &str,
        storage_id: Uuid,
    ) -> PentaractResult<Vec<u8>> {
        // 1. Получаем путь к файлу
        let token = self.scheduler.get_token(storage_id).await?;

        let url = self.build_url("", "getFile", token);

        let body: DownloadBodySchema = http_client()
            .get(url)
            .query(&[("file_id", telegram_file_id)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        // 2. Скачиваем сам файл
        let token = self.scheduler.get_token(storage_id).await?;

        let url = self.build_url("file/", &body.result.file_path, token);

        let response = http_client()
            .get(url)
            .send()
            .await?
            .error_for_status()?;

        let file = response.bytes().await?.to_vec();

        Ok(file)
    }

    /// Создаёт URL Telegram API.
    /// Token передаётся по значению.
    #[inline]
    fn build_url(
        &self,
        pre: &str,
        relative: &str,
        token: String,
    ) -> String {
        format!("{}/{pre}bot{token}/{relative}", self.base_url)
    }
}
