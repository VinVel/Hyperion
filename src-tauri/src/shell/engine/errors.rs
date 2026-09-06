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
    Error as SdkError, HttpError, event_cache::EventCacheError, paginators::PaginatorError,
};
use matrix_sdk_ui::timeline::{Error as TimelineError, PaginationError};

pub(super) fn pagination_error(error: &TimelineError) -> String {
    // Traverse typed sources: live and event-focused pagination wrap SDK errors
    // differently. Formatted server text is never used to classify a rate limit.
    let sdk = match &error {
        TimelineError::EventCacheError(cache) => cache_sdk_error(cache),
        TimelineError::PaginationError(PaginationError::EventCache(cache)) => {
            cache_sdk_error(cache)
        }
        TimelineError::PaginationError(PaginationError::Pagination(PaginatorError::SdkError(
            sdk,
        ))) => Some(sdk.as_ref()),
        _ => None,
    };
    if sdk.is_some_and(sdk_is_rate_limited) {
        return String::from(
            "Older messages are temporarily rate limited. Please wait and try again.",
        );
    }
    format!("Could not load older messages: {error}")
}

fn cache_sdk_error(error: &EventCacheError) -> Option<&SdkError> {
    match error {
        EventCacheError::PaginationError(sdk)
        | EventCacheError::InitialPaginationError(PaginatorError::SdkError(sdk)) => {
            Some(sdk.as_ref())
        }
        _ => None,
    }
}

fn sdk_is_rate_limited(error: &SdkError) -> bool {
    match error {
        SdkError::Http(http) => http_is_rate_limited(http),
        _ => false,
    }
}

fn http_is_rate_limited(error: &HttpError) -> bool {
    if let HttpError::Cached(inner) = error {
        return http_is_rate_limited(inner);
    }
    matches!(
        error.client_api_error_kind(),
        Some(matrix_sdk::ruma::api::error::ErrorKind::LimitExceeded(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn sdk_error(code: &str, message: &str) -> Arc<SdkError> {
        use matrix_sdk::ruma::api::error::{ErrorBody, FromHttpResponseError};
        use matrix_sdk::{Error, HttpError};
        let body: matrix_sdk::ruma::api::error::StandardErrorBody =
            serde_json::from_value(serde_json::json!({
                "errcode": code, "error": message
            }))
            .unwrap();
        let http = HttpError::Api(Box::new(FromHttpResponseError::Server(
            matrix_sdk::ruma::api::client::uiaa::UiaaResponse::MatrixError(
                matrix_sdk::ruma::api::error::Error::new(
                    reqwest::StatusCode::TOO_MANY_REQUESTS,
                    ErrorBody::Standard(body),
                ),
            ),
        )));
        Arc::new(Error::Http(Box::new(http)))
    }

    #[test]
    fn rate_limits_are_classified_through_live_and_focused_wrappers() {
        let sdk = sdk_error("M_LIMIT_EXCEEDED", "slow down");
        let wrappers = [
            TimelineError::EventCacheError(EventCacheError::PaginationError(sdk.clone())),
            TimelineError::PaginationError(PaginationError::Pagination(PaginatorError::SdkError(
                sdk.clone(),
            ))),
            TimelineError::PaginationError(PaginationError::EventCache(
                EventCacheError::PaginationError(sdk.clone()),
            )),
            TimelineError::EventCacheError(EventCacheError::InitialPaginationError(
                PaginatorError::SdkError(sdk),
            )),
        ];
        for error in wrappers {
            assert_eq!(
                pagination_error(&error),
                "Older messages are temporarily rate limited. Please wait and try again."
            );
        }
    }

    #[test]
    fn formatted_error_text_cannot_classify_a_rate_limit() {
        let sdk = sdk_error("M_UNKNOWN", "429 M_LIMIT_EXCEEDED rate limit");
        let error = TimelineError::EventCacheError(EventCacheError::PaginationError(sdk));
        assert!(pagination_error(&error).starts_with("Could not load older messages:"));
    }
}
