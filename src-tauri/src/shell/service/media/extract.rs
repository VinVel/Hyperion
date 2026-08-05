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

use std::time::Duration;

use matrix_sdk::ruma::{
    UInt,
    events::{
        room::{
            MediaSource, ThumbnailInfo,
            message::{
                AudioMessageEventContent, FileMessageEventContent, ImageMessageEventContent,
                MessageType, VideoMessageEventContent,
            },
        },
        sticker::StickerEventContent,
    },
};

use super::{
    registry::ShellMediaService,
    types::{RegisteredMedia, RegisteredMediaFormat},
};
use crate::shell::types::{RoomTimelineAttachment, RoomTimelineAttachmentType};

pub(in crate::shell) fn attachments_from_message_type(
    media_service: &ShellMediaService,
    event_id: &str,
    msgtype: &MessageType,
) -> Vec<RoomTimelineAttachment> {
    match msgtype {
        MessageType::Image(content) => vec![image_attachment(media_service, event_id, content)],
        MessageType::Video(content) => vec![video_attachment(media_service, event_id, content)],
        MessageType::Audio(content) => vec![audio_attachment(media_service, event_id, content)],
        MessageType::File(content) => vec![file_attachment(media_service, event_id, content)],
        _ => Vec::new(),
    }
}

pub(in crate::shell) fn attachments_from_sticker(
    media_service: &ShellMediaService,
    event_id: &str,
    content: &StickerEventContent,
) -> Vec<RoomTimelineAttachment> {
    let source = MediaSource::from(content.source.clone());
    vec![RoomTimelineAttachment {
        event_id: event_id.to_owned(),
        media_type: RoomTimelineAttachmentType::Sticker,
        media_handle: register_file_handle(
            media_service,
            &source,
            None,
            content.info.mimetype.as_deref(),
        ),
        thumbnail_handle: image_like_thumbnail_handle(
            media_service,
            &source,
            content.info.thumbnail_source.as_ref(),
            content.info.thumbnail_info.as_deref(),
        ),
        filename: None,
        display_caption: None,
        mime_type: content.info.mimetype.clone(),
        width: optional_uint_to_u32(content.info.width),
        height: optional_uint_to_u32(content.info.height),
        duration_unix_ms: None,
        size_bytes: optional_uint_to_u64(content.info.size),
        blurhash: content.info.blurhash.clone(),
        requires_reveal: false,
    }]
}

pub(in crate::shell) fn media_body_for_message_type(msgtype: &MessageType) -> Option<String> {
    match msgtype {
        MessageType::Audio(_)
        | MessageType::File(_)
        | MessageType::Image(_)
        | MessageType::Video(_) => Some(String::new()),
        _ => None,
    }
}

fn image_attachment(
    media_service: &ShellMediaService,
    event_id: &str,
    content: &ImageMessageEventContent,
) -> RoomTimelineAttachment {
    let info = content.info.as_deref();
    RoomTimelineAttachment {
        event_id: event_id.to_owned(),
        media_type: RoomTimelineAttachmentType::Image,
        media_handle: register_file_handle(
            media_service,
            &content.source,
            Some(content.filename()),
            info.and_then(|value| value.mimetype.as_deref()),
        ),
        thumbnail_handle: image_like_thumbnail_handle(
            media_service,
            &content.source,
            info.and_then(|value| value.thumbnail_source.as_ref()),
            info.and_then(|value| value.thumbnail_info.as_deref()),
        ),
        filename: None,
        display_caption: caption_text(content.caption()),
        mime_type: info.and_then(|value| value.mimetype.clone()),
        width: info.and_then(|value| optional_uint_to_u32(value.width)),
        height: info.and_then(|value| optional_uint_to_u32(value.height)),
        duration_unix_ms: None,
        size_bytes: info.and_then(|value| optional_uint_to_u64(value.size)),
        blurhash: info.and_then(|value| value.blurhash.clone()),
        requires_reveal: false,
    }
}

fn video_attachment(
    media_service: &ShellMediaService,
    event_id: &str,
    content: &VideoMessageEventContent,
) -> RoomTimelineAttachment {
    let info = content.info.as_deref();
    RoomTimelineAttachment {
        event_id: event_id.to_owned(),
        media_type: RoomTimelineAttachmentType::Video,
        media_handle: register_file_handle(
            media_service,
            &content.source,
            Some(content.filename()),
            info.and_then(|value| value.mimetype.as_deref()),
        ),
        thumbnail_handle: image_like_thumbnail_handle(
            media_service,
            &content.source,
            info.and_then(|value| value.thumbnail_source.as_ref()),
            info.and_then(|value| value.thumbnail_info.as_deref()),
        ),
        filename: None,
        display_caption: caption_text(content.caption()),
        mime_type: info.and_then(|value| value.mimetype.clone()),
        width: info.and_then(|value| optional_uint_to_u32(value.width)),
        height: info.and_then(|value| optional_uint_to_u32(value.height)),
        duration_unix_ms: info.and_then(|value| duration_to_milliseconds(value.duration)),
        size_bytes: info.and_then(|value| optional_uint_to_u64(value.size)),
        blurhash: info.and_then(|value| value.blurhash.clone()),
        requires_reveal: false,
    }
}

fn audio_attachment(
    media_service: &ShellMediaService,
    event_id: &str,
    content: &AudioMessageEventContent,
) -> RoomTimelineAttachment {
    let info = content.info.as_deref();
    RoomTimelineAttachment {
        event_id: event_id.to_owned(),
        media_type: RoomTimelineAttachmentType::Audio,
        media_handle: register_file_handle(
            media_service,
            &content.source,
            Some(content.filename()),
            info.and_then(|value| value.mimetype.as_deref()),
        ),
        thumbnail_handle: None,
        filename: Some(content.filename().to_owned()),
        display_caption: caption_text(content.caption()),
        mime_type: info.and_then(|value| value.mimetype.clone()),
        width: None,
        height: None,
        duration_unix_ms: info.and_then(|value| duration_to_milliseconds(value.duration)),
        size_bytes: info.and_then(|value| optional_uint_to_u64(value.size)),
        blurhash: None,
        requires_reveal: false,
    }
}

fn file_attachment(
    media_service: &ShellMediaService,
    event_id: &str,
    content: &FileMessageEventContent,
) -> RoomTimelineAttachment {
    let info = content.info.as_deref();
    RoomTimelineAttachment {
        event_id: event_id.to_owned(),
        media_type: RoomTimelineAttachmentType::File,
        media_handle: register_file_handle(
            media_service,
            &content.source,
            Some(content.filename()),
            info.and_then(|value| value.mimetype.as_deref()),
        ),
        thumbnail_handle: info.and_then(|value| {
            value.thumbnail_source.as_ref().map(|source| {
                register_thumbnail_handle(media_service, source, value.thumbnail_info.as_deref())
            })
        }),
        filename: Some(content.filename().to_owned()),
        display_caption: caption_text(content.caption()),
        mime_type: info.and_then(|value| value.mimetype.clone()),
        width: None,
        height: None,
        duration_unix_ms: None,
        size_bytes: info.and_then(|value| optional_uint_to_u64(value.size)),
        blurhash: None,
        requires_reveal: false,
    }
}

fn image_like_thumbnail_handle(
    media_service: &ShellMediaService,
    full_source: &MediaSource,
    thumbnail_source: Option<&MediaSource>,
    thumbnail_info: Option<&ThumbnailInfo>,
) -> Option<String> {
    if let Some(source) = thumbnail_source {
        return Some(register_thumbnail_handle(
            media_service,
            source,
            thumbnail_info,
        ));
    }

    // A plain MXC source can be sent to the homeserver thumbnail endpoint.
    // Encrypted originals need a separately encrypted thumbnail source or the
    // SDK would have to download and decrypt the complete file.
    if matches!(full_source, MediaSource::Plain(_)) {
        return Some(register_thumbnail_handle(media_service, full_source, None));
    }

    None
}

fn register_file_handle(
    media_service: &ShellMediaService,
    source: &MediaSource,
    filename: Option<&str>,
    mime_type: Option<&str>,
) -> String {
    media_service.register_media(RegisteredMedia {
        source: source.clone(),
        filename: filename.map(ToOwned::to_owned),
        mime_type: mime_type.map(ToOwned::to_owned),
        format: RegisteredMediaFormat::File,
    })
}

fn register_thumbnail_handle(
    media_service: &ShellMediaService,
    source: &MediaSource,
    info: Option<&ThumbnailInfo>,
) -> String {
    media_service.register_media(RegisteredMedia {
        source: source.clone(),
        filename: None,
        mime_type: info.and_then(|value| value.mimetype.clone()),
        format: RegisteredMediaFormat::Thumbnail,
    })
}

fn caption_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn optional_uint_to_u32(value: Option<UInt>) -> Option<u32> {
    value.and_then(|integer| u32::try_from(u64::from(integer)).ok())
}

fn optional_uint_to_u64(value: Option<UInt>) -> Option<u64> {
    value.map(u64::from)
}

fn duration_to_milliseconds(value: Option<Duration>) -> Option<u64> {
    value.and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

#[cfg(test)]
mod tests {
    use matrix_sdk::ruma::{events::room::MediaSource, owned_mxc_uri};

    use super::*;

    #[test]
    fn image_body_uses_caption_only() {
        let mut content = ImageMessageEventContent::plain(
            String::from("caption"),
            owned_mxc_uri!("mxc://example.org/image"),
        );
        content.filename = Some(String::from("image.png"));

        assert_eq!(
            media_body_for_message_type(&MessageType::Image(content)),
            Some(String::new())
        );
    }

    #[test]
    fn image_body_is_empty_when_body_is_filename() {
        let content = ImageMessageEventContent::new(
            String::from("image.png"),
            MediaSource::Plain(owned_mxc_uri!("mxc://example.org/image")),
        );

        assert_eq!(
            media_body_for_message_type(&MessageType::Image(content)),
            Some(String::new())
        );
    }

    #[test]
    fn plain_media_without_dedicated_thumbnail_uses_homeserver_preview() {
        let media_service = ShellMediaService::new();
        let source = MediaSource::Plain(owned_mxc_uri!("mxc://example.org/image"));

        let handle = image_like_thumbnail_handle(&media_service, &source, None, None)
            .expect("plain MXC media should provide a generated thumbnail handle");
        let media = media_service
            .registered_media(&handle)
            .expect("generated thumbnail handle should be registered");

        assert!(media.format == RegisteredMediaFormat::Thumbnail);
    }
}
