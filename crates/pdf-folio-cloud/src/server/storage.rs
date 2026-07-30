//! Cloudflare R2 presigned URL helpers (SigV4) for the control plane.
//!
//! R2 secret keys never leave the server. Handlers call [`presigned_r2_url`] to
//! mint single-object PUT/GET URLs for content-addressed PDF keys
//! (`blobs/<blake3>.pdf`). Turso credentials are served from config in
//! handlers rather than minted here; this module focuses on object storage.
//!
//! # Invariants
//!
//! - Blob object keys always follow [`r2_blob_key`].
//! - Hashes must be 64-character lowercase/upper hex BLAKE3 digests
//!   ([`validate_hash`]) before a URL is issued.
//! - Presigned URL TTL is short ([`R2_URL_TTL_SECONDS`], ~15 minutes).
//!
//! # Related
//!
//! - [`super::handlers`] — `/token/r2/*` routes
//! - Client: [`crate::sync::blobs::R2Client`]

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use super::config::Config;

type HmacSha256 = Hmac<Sha256>;

/// Lifetime of R2 presigned URLs (~15 minutes).
pub(crate) const R2_URL_TTL_SECONDS: i64 = 60 * 15;

/// Builds an AWS SigV4 query-string presigned URL for R2 (`method` is `GET` or `PUT`).
///
/// # Errors
///
/// Returns an error when the R2 endpoint has no host or HMAC signing fails.
pub(crate) fn presigned_r2_url(
    config: &Config,
    method: &str,
    key: &str,
    expires_seconds: i64,
) -> Result<String> {
    let now = Utc::now();
    let date = now.format("%Y%m%d").to_string();
    let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let region = "auto";
    let service = "s3";
    let credential_scope = format!("{date}/{region}/{service}/aws4_request");
    let credential = format!("{}/{}", config.r2_access_key_id, credential_scope);
    let host = config
        .r2_endpoint
        .host_str()
        .ok_or_else(|| anyhow!("R2 endpoint URL has no host."))?;
    let canonical_uri = format!("/{}/{}", config.r2_bucket, percent_encode_path(key));

    let mut query = [("X-Amz-Algorithm".to_owned(), "AWS4-HMAC-SHA256".to_owned()),
        ("X-Amz-Credential".to_owned(), credential),
        ("X-Amz-Date".to_owned(), timestamp),
        ("X-Amz-Expires".to_owned(), expires_seconds.to_string()),
        ("X-Amz-SignedHeaders".to_owned(), "host".to_owned())];
    query.sort_by(|left, right| left.0.cmp(&right.0));
    let canonical_query = query
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD"
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        now.format("%Y%m%dT%H%M%SZ"),
        credential_scope,
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );
    let signing_key = sigv4_signing_key(&config.r2_secret_access_key, &date, region, service)?;
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes())?);

    let mut url = config.r2_endpoint.clone();
    url.set_path(&format!("{}/{}", config.r2_bucket, key));
    url.set_query(Some(&format!(
        "{canonical_query}&X-Amz-Signature={signature}"
    )));
    Ok(url.to_string())
}

fn sigv4_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Result<Vec<u8>> {
    let date_key = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes())?;
    let region_key = hmac_sha256(&date_key, region.as_bytes())?;
    let service_key = hmac_sha256(&region_key, service.as_bytes())?;
    hmac_sha256(&service_key, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key).context("Could not create HMAC signer.")?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn percent_encode_path(path: &str) -> String {
    path.split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/")
}

fn percent_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
        .replace("%7E", "~")
}

/// Object key for a content-addressed PDF: `blobs/<hash>.pdf`.
pub(crate) fn r2_blob_key(hash: &str) -> String {
    format!("blobs/{hash}.pdf")
}

/// Validates that `hash` is a 64-character hex BLAKE3 digest.
///
/// # Errors
///
/// Returns an error when the string is the wrong length or contains non-hex chars.
/// Callers must validate before presigning so clients cannot request arbitrary keys.
pub(crate) fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(())
    } else {
        bail!("PDF blob hash must be a 64-character hex BLAKE3 digest.")
    }
}
