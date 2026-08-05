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

use tauri::http::{Request, Response, StatusCode, header};

use super::registry::{ServedMedia, ShellMediaService};

// Media elements send standard single-range requests while seeking video/audio.
const BYTES_RANGE_PREFIX: &str = "bytes=";

pub(in crate::shell) fn media_protocol_response(
    media_service: &ShellMediaService,
    request: &Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let token = request.uri().path().trim_start_matches('/');
    let Some(media) = media_service.served_media(token) else {
        return text_response(StatusCode::NOT_FOUND, "Media not found");
    };

    match requested_byte_range(request, media.bytes.len()) {
        Ok(Some(range)) => partial_media_response(media, range),
        Ok(None) => full_media_response(media),
        Err(()) => range_not_satisfiable_response(media.bytes.len()),
    }
}

fn full_media_response(media: ServedMedia) -> Response<Vec<u8>> {
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, media.bytes.len().to_string());
    if let Some(mime_type) = media.mime_type {
        response = response.header(header::CONTENT_TYPE, mime_type);
    }
    response
        .body(media.bytes)
        .expect("media protocol response should build")
}

fn partial_media_response(media: ServedMedia, range: ByteRange) -> Response<Vec<u8>> {
    let body = media.bytes[range.start..=range.end].to_vec();
    let content_range = format!("bytes {}-{}/{}", range.start, range.end, media.bytes.len());
    let mut response = Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, body.len().to_string())
        .header(header::CONTENT_RANGE, content_range);
    if let Some(mime_type) = media.mime_type {
        response = response.header(header::CONTENT_TYPE, mime_type);
    }
    response
        .body(body)
        .expect("media protocol response should build")
}

fn range_not_satisfiable_response(total_len: usize) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{total_len}"))
        .body(Vec::new())
        .expect("media protocol response should build")
}

fn text_response(status: StatusCode, body: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(body.as_bytes().to_vec())
        .expect("media protocol response should build")
}

#[derive(Clone, Copy)]
struct ByteRange {
    start: usize,
    end: usize,
}

fn requested_byte_range(
    request: &Request<Vec<u8>>,
    total_len: usize,
) -> Result<Option<ByteRange>, ()> {
    let Some(header_value) = request.headers().get(header::RANGE) else {
        return Ok(None);
    };
    let range_header = header_value.to_str().map_err(|_| ())?;
    let range_spec = range_header
        .strip_prefix(BYTES_RANGE_PREFIX)
        .ok_or(())?
        .split(',')
        .next()
        .ok_or(())?;
    parse_byte_range(range_spec, total_len).map(Some)
}

fn parse_byte_range(range_spec: &str, total_len: usize) -> Result<ByteRange, ()> {
    if total_len == 0 {
        return Err(());
    }

    let Some((start_part, end_part)) = range_spec.split_once('-') else {
        return Err(());
    };

    if start_part.is_empty() {
        let suffix_len = end_part.parse::<usize>().map_err(|_| ())?;
        if suffix_len == 0 {
            return Err(());
        }
        let start = total_len.saturating_sub(suffix_len);
        return Ok(ByteRange {
            start,
            end: total_len - 1,
        });
    }

    let start = start_part.parse::<usize>().map_err(|_| ())?;
    if start >= total_len {
        return Err(());
    }

    let end = if end_part.is_empty() {
        total_len - 1
    } else {
        end_part
            .parse::<usize>()
            .map_err(|_| ())?
            .min(total_len - 1)
    };

    if end < start {
        return Err(());
    }

    Ok(ByteRange { start, end })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_ended_byte_range() {
        let range = parse_byte_range("3-", 10).unwrap();

        assert_eq!(range.start, 3);
        assert_eq!(range.end, 9);
    }

    #[test]
    fn parses_suffix_byte_range() {
        let range = parse_byte_range("-4", 10).unwrap();

        assert_eq!(range.start, 6);
        assert_eq!(range.end, 9);
    }
}
