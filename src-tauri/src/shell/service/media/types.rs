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

use matrix_sdk::{
    Client,
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::events::room::MediaSource,
};
use serde::{Deserialize, Serialize};

// The thumbnail cache cap follows the product plan while keeping the value
// centralized for future tuning.
pub const DEFAULT_THUMBNAIL_CACHE_ITEM_LIMIT: usize = 100;

// Prepared media is served through an app-local protocol rather than exposed files.
pub const MEDIA_URI_SCHEME: &str = "hyperion-media";

// Timeline previews ask the homeserver for a modest raster so inline rows do
// not decode full-resolution media while scrolling.
pub(super) const TIMELINE_PREVIEW_THUMBNAIL_EDGE_PIXELS: u32 = 480;

#[derive(Clone)]
pub(in crate::shell) struct RegisteredMedia {
    pub source: MediaSource,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub format: RegisteredMediaFormat,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::shell) enum RegisteredMediaFormat {
    File,
    Thumbnail,
}

impl RegisteredMedia {
    pub fn request_parameters(&self) -> MediaRequestParameters {
        MediaRequestParameters {
            source: self.source.clone(),
            format: match self.format {
                RegisteredMediaFormat::File => MediaFormat::File,
                RegisteredMediaFormat::Thumbnail => {
                    MediaFormat::Thumbnail(MediaThumbnailSettings::new(
                        TIMELINE_PREVIEW_THUMBNAIL_EDGE_PIXELS.into(),
                        TIMELINE_PREVIEW_THUMBNAIL_EDGE_PIXELS.into(),
                    ))
                }
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PrepareRoomMediaRequest {
    pub media_handle: String,
}

#[derive(Debug, Serialize)]
pub struct PrepareRoomMediaResponse {
    pub media_url: String,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct SaveRoomMediaRequest {
    pub media_handle: String,
}

#[derive(Debug, Serialize)]
pub struct SaveRoomMediaResponse {
    pub saved: bool,
}

pub(in crate::shell) async fn load_media_bytes(
    client: &Client,
    media: &RegisteredMedia,
) -> Result<Vec<u8>, String> {
    client
        .media()
        .get_media_content(&media.request_parameters(), true)
        .await
        .map_err(|error| format!("Failed to load Matrix media: {error}"))
}
