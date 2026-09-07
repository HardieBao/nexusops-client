use std::{collections::HashSet, net::IpAddr, time::Duration};

use futures::StreamExt;
use reqwest::{header::HeaderValue, Client, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::watch;
use url::Url;

use super::{
    content::{validate_manifest_item, ContentLimits},
    types::{AssetKind, Manifest, ManifestItem, TeamError, TeamProfile},
};

const PROFILE_LIMIT: usize = 1 << 20;
const MANIFEST_LIMIT: usize = 4 << 20;
const DOWNLOAD_LIMIT: usize = 20 << 20;
const MAX_GET_ATTEMPTS: usize = 2;
const RETRY_DELAY: Duration = Duration::from_millis(120);

#[derive(Clone)]
pub struct Cancellation(watch::Sender<bool>);

impl Default for Cancellation {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}

impl Cancellation {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    pub async fn cancelled(&self) {
        let mut receiver = self.0.subscribe();
        while !*receiver.borrow_and_update() {
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
}

#[derive(Clone)]
pub struct TeamApi {
    gateway: Url,
    client: Client,
}

#[derive(Deserialize)]
struct Envelope<T> {
    code: i64,
    data: T,
}

pub fn normalize_gateway(input: &str) -> Result<Url, TeamError> {
    let mut url = checked_url(input.trim())?;
    let path = format!("{}/", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

fn checked_url(input: &str) -> Result<Url, TeamError> {
    let url = Url::parse(input).map_err(|_| TeamError::InvalidGateway)?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(TeamError::InvalidGateway);
    }
    let loopback = url
        .host_str()
        .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback());
    if url.scheme() == "http" && !loopback {
        return Err(TeamError::InsecureGateway);
    }
    Ok(url)
}

impl TeamApi {
    pub fn new(gateway: &str) -> Result<Self, TeamError> {
        Self::with_timeout(gateway, Duration::from_secs(30))
    }

    fn with_timeout(gateway: &str, timeout: Duration) -> Result<Self, TeamError> {
        let client = Client::builder()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10).min(timeout))
            .timeout(timeout)
            .no_proxy()
            .build()
            .map_err(|_| TeamError::Unavailable)?;
        Ok(Self {
            gateway: normalize_gateway(gateway)?,
            client,
        })
    }

    pub fn gateway_url(&self) -> &str {
        self.gateway.as_str()
    }

    fn endpoint(&self, path: &str) -> Result<Url, TeamError> {
        self.gateway.join(path).map_err(|_| TeamError::ForeignUrl)
    }

    // The API returns root-relative canonical paths even behind a reverse-proxy
    // prefix. Resolve only that known path or its exact prefixed equivalent.
    fn checked_reference(&self, reference: &str, canonical: &str) -> Result<Url, TeamError> {
        let expected = self.endpoint(canonical)?;
        let candidate = if reference == canonical || reference == format!("/{canonical}") {
            expected.clone()
        } else if reference.starts_with('/') {
            self.gateway
                .join(reference)
                .map_err(|_| TeamError::ForeignUrl)?
        } else {
            Url::parse(reference).map_err(|_| TeamError::ForeignUrl)?
        };
        if candidate.origin() != self.gateway.origin()
            || candidate.path() != expected.path()
            || !candidate.username().is_empty()
            || candidate.password().is_some()
            || candidate.query().is_some()
            || candidate.fragment().is_some()
        {
            return Err(TeamError::ForeignUrl);
        }
        Ok(expected)
    }

    async fn bytes(
        &self,
        url: Url,
        key: &str,
        limit: usize,
        cancel: &Cancellation,
    ) -> Result<Vec<u8>, TeamError> {
        if key.is_empty() || key.len() > 128 || key.trim() != key {
            return Err(TeamError::AuthenticationRequired);
        }
        let mut credential =
            HeaderValue::from_str(key).map_err(|_| TeamError::AuthenticationRequired)?;
        credential.set_sensitive(true);
        for attempt in 0..MAX_GET_ATTEMPTS {
            let result = tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(TeamError::Cancelled),
                result = self.bytes_once(url.clone(), credential.clone(), limit) => result,
            };
            if !matches!(result, Err(TeamError::Unavailable | TeamError::Timeout))
                || attempt + 1 == MAX_GET_ATTEMPTS
            {
                return result;
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(TeamError::Cancelled),
                _ = tokio::time::sleep(RETRY_DELAY) => {}
            }
        }
        unreachable!("bounded retry loop always returns")
    }

    async fn bytes_once(
        &self,
        url: Url,
        credential: HeaderValue,
        limit: usize,
    ) -> Result<Vec<u8>, TeamError> {
        let response = self
            .client
            .get(url)
            .header("X-Nexus-Member-Key", credential)
            .send()
            .await
            .map_err(transport_error)?;
        if response.status().is_redirection() {
            return Err(TeamError::Redirect);
        }
        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(TeamError::AuthenticationRequired),
            StatusCode::FORBIDDEN => return Err(TeamError::AccessDenied),
            StatusCode::NOT_FOUND => return Err(TeamError::NotFound),
            StatusCode::CONFLICT => return Err(TeamError::Conflict),
            StatusCode::PAYLOAD_TOO_LARGE => return Err(TeamError::TooLarge),
            status if !status.is_success() => return Err(TeamError::Unavailable),
            _ => {}
        }
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(TeamError::TooLarge);
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(transport_error)?;
            if chunk.len() > limit.saturating_sub(body.len()) {
                return Err(TeamError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    async fn json<T: DeserializeOwned>(
        &self,
        path: &str,
        key: &str,
        limit: usize,
        cancel: &Cancellation,
    ) -> Result<T, TeamError> {
        let bytes = self.bytes(self.endpoint(path)?, key, limit, cancel).await?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| TeamError::InvalidResponse)?;
        if contains_credential(&value, key) {
            return Err(TeamError::InvalidResponse);
        }
        let envelope: Envelope<T> =
            serde_json::from_value(value).map_err(|_| TeamError::InvalidResponse)?;
        if envelope.code != 0 {
            return Err(TeamError::InvalidResponse);
        }
        Ok(envelope.data)
    }

    pub async fn profile(
        &self,
        key: &str,
        cancel: &Cancellation,
    ) -> Result<TeamProfile, TeamError> {
        let profile: TeamProfile = self
            .json("api/v1/me/team-profile", key, PROFILE_LIMIT, cancel)
            .await?;
        if profile.schema_version != 1
            || profile.organization_id.is_empty()
            || profile.organization_id.chars().count() > 128
            || profile.workspace_id.is_empty()
            || profile.workspace_id.chars().count() > 128
            || profile.key.id <= 0
            || profile.limits.max_assets == 0
            || profile.limits.max_text_bytes == 0
            || profile.limits.max_archive_bytes == 0
            || profile.limits.max_revisions_per_asset == 0
            || profile.limits.max_unpacked_bytes == 0
            || profile.limits.max_files == 0
            || profile.limits.max_path_depth == 0
            || profile.limits.max_path_bytes == 0
        {
            return Err(TeamError::InvalidResponse);
        }
        checked_url(&profile.base_url)?;
        let advertised_gateway = normalize_gateway(&profile.gateway_url)?;
        if advertised_gateway.origin() != self.gateway.origin()
            || advertised_gateway.path() != self.gateway.path()
        {
            return Err(TeamError::InvalidResponse);
        }
        self.checked_reference(&profile.asset_manifest_url, "api/v1/me/assets/manifest")?;
        Ok(profile)
    }

    pub async fn manifest(&self, key: &str, cancel: &Cancellation) -> Result<Manifest, TeamError> {
        let manifest: Manifest = self
            .json("api/v1/me/assets/manifest", key, MANIFEST_LIMIT, cancel)
            .await?;
        if manifest.schema_version != 1
            || manifest.assets.len() > 10_000
            || manifest.conflicts.len() > 10_000
        {
            return Err(TeamError::InvalidResponse);
        }
        let mut ids = HashSet::new();
        for asset in &manifest.assets {
            if !ids.insert(asset.asset_id) {
                return Err(TeamError::InvalidResponse);
            }
            self.validate_item(asset)?;
        }
        for conflict in &manifest.conflicts {
            if conflict.asset_id <= 0
                || !ids.insert(conflict.asset_id)
                || conflict.reason != "ASSET_RELEASE_CONFLICT"
            {
                return Err(TeamError::InvalidResponse);
            }
        }
        Ok(manifest)
    }

    fn validate_item(&self, asset: &ManifestItem) -> Result<Url, TeamError> {
        validate_manifest_item(asset, ContentLimits::default())
            .map_err(|_| TeamError::InvalidResponse)?;
        if asset.asset_id <= 0
            || asset.revision <= 0
            || !valid_hash(&asset.content_hash)
            || asset.files.len() > 2000
            || (asset.kind == AssetKind::Skill) != asset.archive_sha256.is_some()
            || asset
                .archive_sha256
                .as_deref()
                .is_some_and(|hash| !valid_hash(hash))
        {
            return Err(TeamError::InvalidResponse);
        }
        self.checked_reference(
            &asset.download_url,
            &format!(
                "api/v1/assets/{}/revisions/{}/download",
                asset.asset_id, asset.revision
            ),
        )
    }

    pub async fn download(
        &self,
        asset: &ManifestItem,
        key: &str,
        max_bytes: u64,
        cancel: &Cancellation,
    ) -> Result<Vec<u8>, TeamError> {
        let url = self.validate_item(asset)?;
        let limit = max_bytes.min(DOWNLOAD_LIMIT as u64) as usize;
        if asset.byte_size > limit as u64 {
            return Err(TeamError::TooLarge);
        }
        let body = self.bytes(url, key, limit, cancel).await?;
        if body.len() as u64 != asset.byte_size {
            return Err(TeamError::InvalidResponse);
        }
        if let Some(expected_archive) = asset.archive_sha256.as_ref() {
            if format!("{:x}", Sha256::digest(&body)) != *expected_archive {
                return Err(TeamError::InvalidResponse);
            }
        }
        Ok(body)
    }
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn transport_error(error: reqwest::Error) -> TeamError {
    if error.is_timeout() {
        TeamError::Timeout
    } else {
        TeamError::Unavailable
    }
}

fn contains_credential(value: &serde_json::Value, key: &str) -> bool {
    match value {
        serde_json::Value::String(text) => text.contains(key),
        serde_json::Value::Array(items) => {
            items.iter().any(|value| contains_credential(value, key))
        }
        serde_json::Value::Object(items) => items
            .iter()
            .any(|(name, value)| name.contains(key) || contains_credential(value, key)),
        _ => false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::{
        body::Body,
        response::{IntoResponse, Response},
        routing::get,
        Json, Router,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    pub(crate) async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway = format!("http://{}/", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (gateway, handle)
    }

    #[test]
    fn gateway_prefix_and_transport_policy() {
        assert_eq!(
            normalize_gateway(" https://EXAMPLE.test/edge ")
                .unwrap()
                .as_str(),
            "https://example.test/edge/"
        );
        assert!(normalize_gateway("http://127.0.0.1:18191/gateway").is_ok());
        assert!(normalize_gateway("http://[::1]:18191/gateway").is_ok());
        for url in ["http://example.test", "http://10.0.0.1", "http://localhost"] {
            assert_eq!(
                normalize_gateway(url).unwrap_err(),
                TeamError::InsecureGateway
            );
        }
        for url in [
            "https://user:secret@example.test",
            "https://example.test?key=x",
            "https://example.test/#fragment",
        ] {
            assert_eq!(
                normalize_gateway(url).unwrap_err(),
                TeamError::InvalidGateway
            );
        }
    }

    #[test]
    fn only_exact_same_origin_managed_download_paths_are_allowed() {
        let api = TeamApi::new("https://example.test/edge/").unwrap();
        let path = "api/v1/assets/1/revisions/2/download";
        for reference in [
            format!("/{path}"),
            path.to_owned(),
            format!("/edge/{path}"),
            format!("https://example.test/edge/{path}"),
        ] {
            assert_eq!(
                api.checked_reference(&reference, path).unwrap().path(),
                format!("/edge/{path}")
            );
        }
        for reference in [
            "https://other.test/edge/api/v1/assets/1/revisions/2/download",
            "https://example.test/api/v1/admin/settings",
            "https://example.test/edge/api/v1/assets/1/revisions/2/download?key=secret",
            "https://user:secret@example.test/edge/api/v1/assets/1/revisions/2/download",
            "https://example.test/edge/api/v1/assets/1/revisions/2/download#fragment",
            "https://example.test/edge/api/v1/assets/1/revisions/3/download",
        ] {
            assert_eq!(
                api.checked_reference(reference, path).unwrap_err(),
                TeamError::ForeignUrl
            );
        }
        let invalid_skill = ManifestItem {
            asset_id: 1,
            kind: AssetKind::Skill,
            slug: "missing-files".into(),
            name: "Missing files".into(),
            revision: 2,
            content_hash: "0".repeat(64),
            archive_sha256: Some("1".repeat(64)),
            download_url: "/edge/api/v1/assets/1/revisions/2/download".into(),
            content_type: "application/zip".into(),
            byte_size: 10,
            files: Vec::new(),
        };
        assert_eq!(
            api.validate_item(&invalid_skill).unwrap_err(),
            TeamError::InvalidResponse
        );
    }

    #[tokio::test]
    async fn redirects_never_forward_the_member_key() {
        let target_calls = Arc::new(AtomicUsize::new(0));
        let target_counter = target_calls.clone();
        let (target, target_handle) = serve(Router::new().route(
            "/",
            get(move || async move {
                target_counter.fetch_add(1, Ordering::SeqCst);
                StatusCode::OK
            }),
        ))
        .await;
        let (gateway, handle) = serve(Router::new().route(
            "/api/v1/me/team-profile",
            get(move || async move {
                Response::builder()
                    .status(StatusCode::FOUND)
                    .header("Location", target)
                    .body(Body::empty())
                    .unwrap()
            }),
        ))
        .await;
        let result = TeamApi::new(&gateway)
            .unwrap()
            .profile("nx_synthetic_fixture", &Cancellation::default())
            .await;
        assert_eq!(result.unwrap_err(), TeamError::Redirect);
        assert_eq!(target_calls.load(Ordering::SeqCst), 0);
        handle.abort();
        target_handle.abort();
    }

    #[tokio::test]
    async fn transient_get_is_retried_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let profile = Arc::new(std::sync::Mutex::new(super::super::types::fixture_profile()));
        let response = profile.clone();
        let (gateway, handle) = serve(Router::new().route(
            "/api/v1/me/team-profile",
            get(move || {
                let counter = counter.clone();
                async move {
                    if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                        return StatusCode::SERVICE_UNAVAILABLE.into_response();
                    }
                    Json(serde_json::json!({
                        "code": 0,
                        "data": response.lock().unwrap().clone()
                    }))
                    .into_response()
                }
            }),
        ))
        .await;
        profile.lock().unwrap().gateway_url = gateway.clone();

        TeamApi::new(&gateway)
            .unwrap()
            .profile("nx_synthetic_fixture", &Cancellation::default())
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        handle.abort();
    }

    #[tokio::test]
    async fn unsupported_profile_and_manifest_versions_are_rejected() {
        let mut profile = super::super::types::fixture_profile();
        profile.schema_version = 2;
        let manifest = Manifest {
            schema_version: 2,
            assets: Vec::new(),
            conflicts: Vec::new(),
        };
        let (gateway, handle) = serve(
            Router::new()
                .route(
                    "/api/v1/me/team-profile",
                    get(move || {
                        let profile = profile.clone();
                        async move { Json(serde_json::json!({ "code": 0, "data": profile })) }
                    }),
                )
                .route(
                    "/api/v1/me/assets/manifest",
                    get(move || {
                        let manifest = manifest.clone();
                        async move { Json(serde_json::json!({ "code": 0, "data": manifest })) }
                    }),
                ),
        )
        .await;
        let api = TeamApi::new(&gateway).unwrap();

        assert_eq!(
            api.profile("nx_synthetic_fixture", &Cancellation::default())
                .await
                .unwrap_err(),
            TeamError::InvalidResponse
        );
        assert_eq!(
            api.manifest("nx_synthetic_fixture", &Cancellation::default())
                .await
                .unwrap_err(),
            TeamError::InvalidResponse
        );
        handle.abort();
    }

    #[tokio::test]
    async fn text_download_defers_bom_and_crlf_hashing_to_content_verification() {
        let body = b"\xef\xbb\xbfa\r\n".to_vec();
        let served = body.clone();
        let (gateway, handle) = serve(Router::new().route(
            "/api/v1/assets/9/revisions/3/download",
            get(move || {
                let served = served.clone();
                async move { Body::from(served) }
            }),
        ))
        .await;
        let item = ManifestItem {
            asset_id: 9,
            kind: AssetKind::Prompt,
            slug: "normalized-text".into(),
            name: "Normalized text".into(),
            revision: 3,
            content_hash: format!("{:x}", Sha256::digest(b"a\n")),
            archive_sha256: None,
            download_url: "/api/v1/assets/9/revisions/3/download".into(),
            content_type: "text/plain; charset=utf-8".into(),
            byte_size: body.len() as u64,
            files: Vec::new(),
        };
        let downloaded = TeamApi::new(&gateway)
            .unwrap()
            .download(
                &item,
                "nx_synthetic_fixture",
                1024,
                &Cancellation::default(),
            )
            .await
            .unwrap();
        assert_eq!(downloaded, body);
        let verified = super::super::content::verify_download(
            &item,
            &downloaded,
            super::super::content::ContentLimits::default(),
        )
        .unwrap();
        assert_eq!(verified.body, b"a\n");
        handle.abort();
    }

    #[tokio::test]
    async fn http_errors_limits_timeout_and_cancellation_are_explicit() {
        for (status, expected) in [
            (401, TeamError::AuthenticationRequired),
            (403, TeamError::AccessDenied),
            (404, TeamError::NotFound),
            (409, TeamError::Conflict),
        ] {
            let (gateway, handle) = serve(Router::new().route(
                "/api/v1/me/team-profile",
                get(move || async move {
                    Response::builder()
                        .status(status)
                        .body(Body::from("untrusted response must never appear in errors"))
                        .unwrap()
                }),
            ))
            .await;
            assert_eq!(
                TeamApi::new(&gateway)
                    .unwrap()
                    .profile("nx_synthetic_fixture", &Cancellation::default())
                    .await
                    .unwrap_err(),
                expected
            );
            handle.abort();
        }
        let (gateway, handle) = serve(Router::new().route(
            "/api/v1/me/team-profile",
            get(|| async {
                Body::from_stream(futures::stream::iter(vec![
                    Ok::<_, std::convert::Infallible>(bytes::Bytes::from(vec![
                        b'x';
                        PROFILE_LIMIT + 1
                    ])),
                ]))
            }),
        ))
        .await;
        assert_eq!(
            TeamApi::new(&gateway)
                .unwrap()
                .profile("nx_synthetic_fixture", &Cancellation::default())
                .await
                .unwrap_err(),
            TeamError::TooLarge
        );
        handle.abort();

        let (gateway, handle) = serve(Router::new().route(
            "/api/v1/me/team-profile",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                Json(serde_json::json!({"code":0,"data":{}}))
            }),
        ))
        .await;
        assert_eq!(
            TeamApi::with_timeout(&gateway, Duration::from_millis(30))
                .unwrap()
                .profile("nx_synthetic_fixture", &Cancellation::default())
                .await
                .unwrap_err(),
            TeamError::Timeout
        );
        let cancel = Cancellation::default();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            trigger.cancel();
        });
        assert_eq!(
            TeamApi::new(&gateway)
                .unwrap()
                .profile("nx_synthetic_fixture", &cancel)
                .await
                .unwrap_err(),
            TeamError::Cancelled
        );
        handle.abort();
    }

    #[test]
    fn reflected_credentials_are_rejected_before_caching() {
        assert!(contains_credential(
            &serde_json::json!({"data":{"name":"prefix nx_secret suffix"}}),
            "nx_secret"
        ));
        assert!(!contains_credential(
            &serde_json::json!({"data":{"key":{"prefix":"nx_"}}}),
            "nx_secret"
        ));
    }
}
