/*
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

use tauri::http::{Request, Response};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_fs::{FsExt, OpenOptions};

use super::{
    cache::{cached_thumbnail, remember_thumbnail},
    protocol::media_protocol_response,
    registry::ShellMediaService,
    types::{
        PrepareRoomMediaRequest, PrepareRoomMediaResponse, RegisteredMediaFormat,
        SaveRoomMediaRequest, SaveRoomMediaResponse, load_media_bytes,
    },
};
use crate::{account::ActiveAccount, shell::service::ShellManager};

impl ShellManager {
    pub async fn prepare_room_media(
        &self,
        active_account: &ActiveAccount,
        request: PrepareRoomMediaRequest,
    ) -> Result<PrepareRoomMediaResponse, String> {
        self.media_service
            .prepare_room_media(active_account, request)
            .await
    }

    pub async fn save_room_media(
        &self,
        app: &crate::AppHandle,
        active_account: &ActiveAccount,
        request: SaveRoomMediaRequest,
    ) -> Result<SaveRoomMediaResponse, String> {
        self.media_service
            .save_room_media(app, active_account, request)
            .await
    }

    pub fn media_protocol_response(&self, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
        media_protocol_response(&self.media_service, request)
    }
}

impl ShellMediaService {
    async fn prepare_room_media(
        &self,
        active_account: &ActiveAccount,
        request: PrepareRoomMediaRequest,
    ) -> Result<PrepareRoomMediaResponse, String> {
        let account = active_account.snapshot();
        let media = self.registered_media(&request.media_handle)?;
        let bytes = if media.format == RegisteredMediaFormat::Thumbnail {
            if let Some((bytes, mime_type)) =
                cached_thumbnail_async(account.store_dir.clone(), request.media_handle.clone())
                    .await?
            {
                return Ok(self.prepare_served_media_response(bytes, mime_type.as_deref()));
            }

            let bytes = load_media_bytes(&account.client, &media).await?;
            let resolved_mime_type = resolve_mime_type(media.mime_type.as_deref(), &bytes);
            remember_thumbnail_async(
                account.store_dir.clone(),
                request.media_handle.clone(),
                bytes,
                resolved_mime_type,
            )
            .await?
        } else {
            load_media_bytes(&account.client, &media).await?
        };

        Ok(self.prepare_served_media_response(bytes, media.mime_type.as_deref()))
    }

    async fn save_room_media(
        &self,
        app: &crate::AppHandle,
        active_account: &ActiveAccount,
        request: SaveRoomMediaRequest,
    ) -> Result<SaveRoomMediaResponse, String> {
        let account = active_account.snapshot();
        let media = self.registered_media(&request.media_handle)?;
        let Some(save_path) = app
            .dialog()
            .file()
            .set_file_name(media.filename.as_deref().unwrap_or("media"))
            .blocking_save_file()
        else {
            return Ok(SaveRoomMediaResponse { saved: false });
        };

        let bytes = load_media_bytes(&account.client, &media).await?;
        let path = dialog_file_path(save_path)?;
        let mut open_options = OpenOptions::new();
        open_options.write(true).create(true).truncate(true);
        let mut file = app
            .fs()
            .open(path, open_options)
            .map_err(|error| format!("Failed to open media destination: {error}"))?;
        std::io::Write::write_all(&mut file, &bytes)
            .map_err(|error| format!("Failed to save media file: {error}"))?;
        Ok(SaveRoomMediaResponse { saved: true })
    }

    fn prepare_served_media_response(
        &self,
        bytes: Vec<u8>,
        metadata_mime_type: Option<&str>,
    ) -> PrepareRoomMediaResponse {
        let size_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let mime_type = resolve_mime_type(metadata_mime_type, &bytes);
        let served_token = self.store_served_media(bytes, mime_type.clone());
        PrepareRoomMediaResponse {
            media_url: Self::media_url(&served_token),
            mime_type,
            size_bytes,
        }
    }
}

async fn cached_thumbnail_async(
    store_dir: std::path::PathBuf,
    media_handle: String,
) -> Result<Option<(Vec<u8>, Option<String>)>, String> {
    tauri::async_runtime::spawn_blocking(move || cached_thumbnail(&store_dir, &media_handle))
        .await
        .map_err(|error| format!("Failed to join thumbnail cache read: {error}"))?
}

async fn remember_thumbnail_async(
    store_dir: std::path::PathBuf,
    media_handle: String,
    bytes: Vec<u8>,
    mime_type: Option<String>,
) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        remember_thumbnail(&store_dir, &media_handle, &bytes, mime_type.as_deref())?;
        Ok(bytes)
    })
    .await
    .map_err(|error| format!("Failed to join thumbnail cache write: {error}"))?
}

fn dialog_file_path(file_path: FilePath) -> Result<std::path::PathBuf, String> {
    match file_path {
        FilePath::Path(path) => Ok(path),
        FilePath::Url(url) => Err(format!("Unsupported media save URL: {url}")),
    }
}

fn resolve_mime_type(metadata_mime_type: Option<&str>, bytes: &[u8]) -> Option<String> {
    metadata_mime_type
        .filter(|mime_type| !mime_type.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| sniff_mime_type(bytes).map(ToOwned::to_owned))
}

fn sniff_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1A\n") {
        return Some("image/png");
    }

    if bytes.starts_with(b"\xFF\xD8\xFF") {
        return Some("image/jpeg");
    }

    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }

    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP" {
        return Some("image/webp");
    }

    if bytes.len() >= 12 && bytes[4..8] == *b"ftyp" {
        return Some("video/mp4");
    }

    if bytes.starts_with(b"\x1A\x45\xDF\xA3") {
        return Some("video/webm");
    }

    if bytes.starts_with(b"OggS") {
        return Some("audio/ogg");
    }

    if bytes.starts_with(b"ID3") || bytes.starts_with(b"\xFF\xFB") {
        return Some("audio/mpeg");
    }

    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WAVE" {
        return Some("audio/wav");
    }

    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf");
    }

    None
}
