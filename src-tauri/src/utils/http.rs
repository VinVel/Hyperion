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

#[cfg(target_os = "android")]
use std::sync::Arc;

use reqwest::Client as HttpClient;
#[cfg(target_os = "android")]
use rustls::{ClientConfig, RootCertStore, client::WebPkiServerVerifier};

#[cfg(target_os = "android")]
pub(crate) fn external_http_client() -> Result<HttpClient, String> {
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let native_certs = rustls_native_certs::load_native_certs().certs;
    root_store.add_parsable_certificates(native_certs);

    // Direct reqwest clients can otherwise route through rustls-platform-verifier,
    // whose Android verifier requires app-context initialization outside this path.
    let verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
        .build()
        .map_err(|error| format!("Failed to configure Android TLS verifier: {error}"))?;
    let tls_config = ClientConfig::builder()
        .with_webpki_verifier(verifier)
        .with_no_client_auth();

    HttpClient::builder()
        .tls_backend_preconfigured(tls_config)
        .build()
        .map_err(|error| format!("Failed to build Android HTTP client: {error}"))
}

#[cfg(not(target_os = "android"))]
pub(crate) fn external_http_client() -> Result<HttpClient, String> {
    HttpClient::builder()
        .build()
        .map_err(|error| format!("Failed to build HTTP client: {error}"))
}
