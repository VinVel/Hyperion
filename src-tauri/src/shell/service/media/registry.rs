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

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use matrix_sdk::ruma::events::room::MediaSource;

use super::types::{
    MEDIA_URI_SCHEME, RegisteredMedia, RegisteredMediaFormat,
    TIMELINE_PREVIEW_THUMBNAIL_EDGE_PIXELS,
};

// Served media remains bounded because prepared videos and images can be large
// and custom-protocol URLs otherwise keep their backing bytes for the app lifetime.
const SERVED_MEDIA_ITEM_LIMIT: usize = 64;

#[derive(Clone)]
pub struct ServedMedia {
    pub bytes: Vec<u8>,
    pub mime_type: Option<String>,
}

#[derive(Default)]
struct MediaRegistryState {
    registered_media: HashMap<String, RegisteredMedia>,
    served_media: HashMap<String, ServedMedia>,
    served_media_order: VecDeque<String>,
}

#[derive(Clone, Default)]
pub(in crate::shell) struct ShellMediaService {
    state: Arc<Mutex<MediaRegistryState>>,
}

impl ShellMediaService {
    pub(in crate::shell) fn new() -> Self {
        Self::default()
    }

    pub(in crate::shell) fn register_media(&self, media: RegisteredMedia) -> String {
        let token = media_token(
            &media.source,
            media.format == RegisteredMediaFormat::Thumbnail,
        );
        let mut state = self
            .state
            .lock()
            .expect("media registry lock should not be poisoned");
        state.registered_media.entry(token.clone()).or_insert(media);
        token
    }

    pub(in crate::shell) fn registered_media(
        &self,
        media_handle: &str,
    ) -> Result<RegisteredMedia, String> {
        self.state
            .lock()
            .expect("media registry lock should not be poisoned")
            .registered_media
            .get(media_handle)
            .cloned()
            .ok_or_else(|| String::from("Unknown media handle"))
    }

    pub(in crate::shell) fn store_served_media(
        &self,
        bytes: Vec<u8>,
        mime_type: Option<String>,
    ) -> String {
        let token = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 18]>());
        let mut state = self
            .state
            .lock()
            .expect("served media lock should not be poisoned");
        state
            .served_media
            .insert(token.clone(), ServedMedia { bytes, mime_type });
        state.served_media_order.push_back(token.clone());
        while state.served_media_order.len() > SERVED_MEDIA_ITEM_LIMIT {
            if let Some(expired_token) = state.served_media_order.pop_front() {
                state.served_media.remove(&expired_token);
            }
        }
        drop(state);
        token
    }

    pub(in crate::shell) fn served_media(&self, token: &str) -> Option<ServedMedia> {
        self.state
            .lock()
            .expect("served media lock should not be poisoned")
            .served_media
            .get(token)
            .cloned()
    }

    pub(in crate::shell) fn clear(&self) {
        let mut state = self
            .state
            .lock()
            .expect("media registry lock should not be poisoned");
        state.registered_media.clear();
        state.served_media.clear();
        state.served_media_order.clear();
    }

    pub(in crate::shell) fn media_url(served_token: &str) -> String {
        #[cfg(any(target_os = "windows", target_os = "android"))]
        {
            format!("http://{MEDIA_URI_SCHEME}.localhost/{served_token}")
        }

        #[cfg(not(any(target_os = "windows", target_os = "android")))]
        format!("{MEDIA_URI_SCHEME}://localhost/{served_token}")
    }
}

fn media_token(source: &MediaSource, is_thumbnail: bool) -> String {
    let source_label = match source {
        MediaSource::Plain(uri) => uri.to_string(),
        MediaSource::Encrypted(file) => file.url.to_string(),
    };
    // The requested preview edge participates in the handle so a size change
    // cannot reuse larger thumbnail bytes persisted by an older build.
    let prefix = if is_thumbnail {
        format!("thumb-{TIMELINE_PREVIEW_THUMBNAIL_EDGE_PIXELS}")
    } else {
        String::from("file")
    };
    format!("{prefix}-{}", URL_SAFE_NO_PAD.encode(source_label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn served_media_registry_stays_bounded() {
        let media_service = ShellMediaService::new();
        let mut tokens = Vec::new();
        for index in 0..=SERVED_MEDIA_ITEM_LIMIT {
            let byte = u8::try_from(index).expect("test registry limit should fit in one byte");
            tokens.push(media_service.store_served_media(vec![byte], None));
        }

        assert!(media_service.served_media(&tokens[0]).is_none());
        assert!(
            media_service
                .served_media(tokens.last().expect("latest token should exist"))
                .is_some()
        );
    }

    #[test]
    fn clearing_media_service_removes_registered_and_served_media() {
        let media_service = ShellMediaService::new();
        let served_token = media_service.store_served_media(vec![1], None);

        media_service.clear();

        assert!(media_service.served_media(&served_token).is_none());
    }
}
