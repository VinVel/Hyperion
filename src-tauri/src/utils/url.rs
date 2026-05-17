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

use std::fmt::Write;

// Homeserver directory entries can omit a scheme; default to HTTPS before
// handing the URL to Matrix SDK builders or external browsers.
pub(crate) fn ensure_https_url(value: &str) -> String {
    if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    }
}

// Percent-encode a single URL path segment while preserving RFC 3986
// unreserved bytes.
pub(crate) fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();

    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }

    encoded
}
