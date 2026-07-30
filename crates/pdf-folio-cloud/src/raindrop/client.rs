//! Raindrop.io HTTP client, remote response models, and SSRF-safe PDF downloads.
//!
//! [`RaindropClient`] is the private transport for list/user/collection calls
//! and for downloading individual PDFs or the bulk ZIP export. Download paths
//! enforce host validation, private/reserved IP blocking, redirect limits, size
//! caps, and a PDF magic-byte check so import cannot be used as an open proxy
//! into the local network.
//!
//! # Security notes (PDF download)
//!
//! - Only `http`/`https`; blocks localhost and private/link-local/reserved ranges
//! - Manual redirect following with re-validation of each hop
//! - Caps at 256 MiB and 120s timeout; requires `%PDF-` in the first 1 KiB
//! - Raindrop-hosted cache URLs send the bearer token; third-party links do not
//!
//! # Related
//!
//! - Auth token: [`super::auth`]
//! - Orchestration: [`super::import`]
//! - ZIP matching: [`super::matching`]

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::header::{AUTHORIZATION, LOCATION};
use serde::Deserialize;
use url::Url;

use super::{
    RaindropPdfCandidate, API_BASE, MAX_PER_PAGE, ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS,
    ZIP_PREPARING_PROGRESS_BASIS_POINTS,
};

/// Hard cap on a single PDF download body (256 MiB).
const MAX_PDF_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
/// Maximum HTTP redirects followed while downloading a PDF.
const MAX_PDF_REDIRECTS: usize = 10;
/// Wall-clock timeout for one PDF download attempt (including redirects).
const PDF_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

impl RaindropPdfCandidate {
    /// Rebuilds a private [`Raindrop`] view from a public candidate (for download helpers).
    pub(crate) fn to_raindrop(&self) -> Raindrop {
        Raindrop {
            id: self.id,
            title: Some(self.title.clone()),
            link: self.download_link.clone(),
            cover: self.thumbnail_url.clone(),
            media: Vec::new(),
            item_type: Some(String::from("document")),
            collection: self.collection_id.map(|id| RaindropRef { id }),
            tags: self.tags.clone(),
            file: Some(RaindropFile {
                name: self.file_name.clone(),
                link: Some(self.download_link.clone()),
                size: self.file_size,
                mime_type: Some(String::from("application/pdf")),
            }),
            last_update: self.remote_updated_at.clone(),
            uploaded_file: self.uploaded_file,
        }
    }
}

/// Maps ZIP byte progress into the download phase of the basis-point scale.
pub(crate) fn zip_download_progress_basis_points(downloaded: u64, total: u64) -> u16 {
    if total == 0 {
        return ZIP_PREPARING_PROGRESS_BASIS_POINTS;
    }

    let base = u64::from(ZIP_PREPARING_PROGRESS_BASIS_POINTS);
    let downloading =
        u64::from(ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS - ZIP_PREPARING_PROGRESS_BASIS_POINTS);
    (base + downloading * downloaded.min(total) / total)
        .min(u64::from(ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS)) as u16
}

/// Authenticated Raindrop REST client (`Authorization: Bearer`).
pub(crate) struct RaindropClient {
    /// Shared HTTP client (user agent set at construction).
    http: reqwest::Client,
    /// OAuth access token sent on API and Raindrop-hosted download requests.
    token: String,
}

impl RaindropClient {
    /// Builds a client with the PDF-Folio Raindrop user agent.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying `reqwest` client cannot be built.
    pub(crate) fn new(token: String) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent("PDF-Folio Raindrop Import")
                .build()?,
            token,
        })
    }

    /// Fetches the authenticated Raindrop user profile.
    pub(crate) async fn user(&self) -> Result<RaindropUser> {
        let response = self
            .get_json::<UserResponse>(&format!("{API_BASE}/user"))
            .await?;
        Ok(response.user)
    }

    /// Lists root and child collections (deduped, sorted by parent then sort key).
    pub(crate) async fn collections(&self) -> Result<Vec<RaindropCollection>> {
        let mut collections = self
            .get_json::<CollectionsResponse>(&format!("{API_BASE}/collections"))
            .await?
            .items;
        collections.extend(
            self.get_json::<CollectionsResponse>(&format!("{API_BASE}/collections/childrens"))
                .await?
                .items,
        );
        collections
            .sort_by_key(|collection| (collection.parent_id().unwrap_or(0), collection.sort));
        collections.dedup_by_key(|collection| collection.id);
        Ok(collections)
    }

    /// Fetches a single raindrop by id.
    pub(crate) async fn raindrop(&self, id: i64) -> Result<Raindrop> {
        let response = self
            .get_json::<RaindropResponse>(&format!("{API_BASE}/raindrop/{id}"))
            .await?;
        Ok(response.item)
    }

    /// Pages all raindrops and keeps those that look like PDFs ([`Raindrop::is_pdf`]).
    pub(crate) async fn pdf_raindrops(&self) -> Result<Vec<Raindrop>> {
        let mut page = 0_u32;
        let mut raindrops = Vec::new();

        loop {
            let mut url = Url::parse(&format!("{API_BASE}/raindrops/0"))?;
            url.query_pairs_mut()
                .append_pair("page", &page.to_string())
                .append_pair("perpage", &MAX_PER_PAGE.to_string())
                .append_pair("sort", "-created");
            let response = self.get_json::<RaindropsResponse>(url.as_str()).await?;
            let count = response.items.len();
            raindrops.extend(response.items.into_iter().filter(Raindrop::is_pdf));
            if count < usize::from(MAX_PER_PAGE) {
                break;
            }
            page += 1;
        }

        Ok(raindrops)
    }

    /// Downloads a PDF from `link` with SSRF guards, size limit, and timeout.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure, blocked addresses, oversize body,
    /// timeout, or non-PDF content.
    pub(crate) async fn download_pdf(&self, link: &str) -> Result<Vec<u8>> {
        tokio::time::timeout(PDF_DOWNLOAD_TIMEOUT, self.download_pdf_with_guards(link))
            .await
            .with_context(|| format!("Timed out downloading PDF from {link}."))?
    }

    async fn download_pdf_with_guards(&self, link: &str) -> Result<Vec<u8>> {
        let mut target = validated_pdf_download_target(link).await?;

        for redirects in 0..=MAX_PDF_REDIRECTS {
            let download_http = guarded_pdf_download_client(&target)?;
            let mut request = download_http
                .get(target.url.clone())
                .header("Accept", "application/pdf,*/*");
            if download_requires_raindrop_auth(target.url.as_str()) {
                request = request.header(AUTHORIZATION, format!("Bearer {}", self.token));
            }
            let response = request.send().await?;

            if response.status().is_redirection() {
                if redirects == MAX_PDF_REDIRECTS {
                    bail!("Too many redirects while downloading PDF from {link}.");
                }
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|location| location.to_str().ok())
                    .ok_or_else(|| {
                        anyhow!("Redirect while downloading PDF from {link} did not include a location.")
                    })?;
                let next_url = target.url.join(location).with_context(|| {
                    format!("Redirect while downloading PDF from {link} had an invalid location.")
                })?;
                target = validate_pdf_download_url(next_url).await?;
                continue;
            }

            let response = response.error_for_status()?;
            return read_limited_pdf_response(target.url.as_str(), response).await;
        }

        unreachable!("redirect loop exits by returning or bailing")
    }

    /// Downloads a raindrop’s PDF (Raindrop cache API for uploads, else download link).
    pub(crate) async fn download_pdf_for_raindrop(&self, raindrop: &Raindrop) -> Result<Vec<u8>> {
        if raindrop.has_uploaded_file() {
            return self
                .download_pdf(&format!("{API_BASE}/raindrop/{}/cache", raindrop.id))
                .await;
        }

        self.download_pdf(raindrop.download_link()).await
    }

    /// Downloads Raindrop’s bulk ZIP export of uploaded files (`file:true` search).
    ///
    /// Invokes `progress` with basis points during download. Response must start
    /// with ZIP magic (`PK`).
    pub(crate) async fn download_pdf_export_zip(
        &self,
        mut progress: impl FnMut(u16),
    ) -> Result<Vec<u8>> {
        let mut url = Url::parse(&format!("{API_BASE}/raindrops/0/export.zip"))?;
        url.query_pairs_mut().append_pair("search", "file:true");
        let response = self
            .http
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header("Accept", "application/zip,application/octet-stream,*/*")
            .send()
            .await?
            .error_for_status()?;
        let content_length = response.content_length();
        progress(ZIP_PREPARING_PROGRESS_BASIS_POINTS);

        let mut bytes = Vec::with_capacity(
            content_length
                .and_then(|length| length.try_into().ok())
                .unwrap_or_default(),
        );
        let mut downloaded = 0_u64;
        let mut response = response;
        while let Some(chunk) = response.chunk().await? {
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            bytes.extend_from_slice(&chunk);
            if let Some(content_length) = content_length {
                progress(zip_download_progress_basis_points(
                    downloaded,
                    content_length,
                ));
            } else {
                progress(ZIP_PREPARING_PROGRESS_BASIS_POINTS);
            }
        }
        progress(ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS);
        if !bytes.starts_with(b"PK") {
            let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(160)])
                .replace(['\n', '\r'], " ");
            bail!("Raindrop export response was not a ZIP archive. Response preview: {preview}");
        }
        Ok(bytes)
    }

    async fn get_json<T>(&self, url: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let body = self
            .http
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await
            .with_context(|| format!("Could not read Raindrop response from {url}."))?;
        serde_json::from_str::<T>(&body).map_err(|error| {
            let preview = body.chars().take(240).collect::<String>();
            anyhow!("Could not decode Raindrop response from {url}: {error}. Response preview: {preview}")
        })
    }
}

/// True when the URL host is `raindrop.io` (or a subdomain) so the bearer token should be sent.
fn download_requires_raindrop_auth(link: &str) -> bool {
    Url::parse(link)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "raindrop.io" || host.ends_with(".raindrop.io"))
}

/// Resolved download target after scheme/host/DNS allow-list checks.
#[derive(Debug)]
pub(super) struct PdfDownloadTarget {
    /// Validated absolute download URL (after redirects this is the current hop).
    url: Url,
    /// Hostname used for DNS pinning on the guarded client.
    host: String,
    /// Resolved public addresses only (private/reserved filtered out).
    addresses: Vec<SocketAddr>,
}

/// Parses and validates a PDF download URL (public entry for tests / helpers).
pub(super) async fn validated_pdf_download_target(link: &str) -> Result<PdfDownloadTarget> {
    let url = Url::parse(link).with_context(|| format!("Invalid PDF download URL: {link}"))?;
    validate_pdf_download_url(url).await
}

/// Validates scheme/host, resolves DNS, and rejects private/reserved addresses.
///
/// # Errors
///
/// Returns an error for unsupported schemes, localhost, resolution failure, or blocked IPs.
async fn validate_pdf_download_url(url: Url) -> Result<PdfDownloadTarget> {
    match url.scheme() {
        "http" | "https" => {}
        scheme => bail!("Refusing to download PDF from unsupported URL scheme: {scheme}."),
    }

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("Refusing to download PDF from a URL without a host."))?;
    if host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".localhost") {
        bail!("Refusing to download PDF from local host: {host}.");
    }
    let host = host.to_owned();

    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow!("Could not determine port for PDF download URL: {url}"))?;
    let addresses = tokio::net::lookup_host((host.as_str(), port))
        .await
        .with_context(|| format!("Could not resolve PDF download host: {host}"))?;
    let mut allowed_addresses = Vec::new();
    for address in addresses {
        let ip = normalize_download_address(address.ip());
        if is_blocked_download_address(ip) {
            bail!("Refusing to download PDF from local or private address: {ip}.");
        }
        allowed_addresses.push(address);
    }
    if allowed_addresses.is_empty() {
        bail!("PDF download host did not resolve: {host}.");
    }

    Ok(PdfDownloadTarget {
        url,
        host,
        addresses: allowed_addresses,
    })
}

/// Builds a `reqwest` client that disables auto-redirects and pins DNS to pre-validated addresses.
///
/// # Errors
///
/// Returns an error when the client cannot be constructed.
fn guarded_pdf_download_client(target: &PdfDownloadTarget) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("PDF-Folio Raindrop Import")
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&target.host, &target.addresses)
        .build()
        .context("Could not build guarded PDF download client.")
}

/// Unwraps IPv4-mapped IPv6 addresses so private-range checks apply to the v4 form.
fn normalize_download_address(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ip)),
        ip => ip,
    }
}

/// Returns true when `ip` is loopback, private, link-local, or otherwise blocked for downloads.
pub(super) fn is_blocked_download_address(ip: IpAddr) -> bool {
    match normalize_download_address(ip) {
        IpAddr::V4(ip) => is_blocked_ipv4_address(ip),
        IpAddr::V6(ip) => is_blocked_ipv6_address(ip),
    }
}

/// IPv4 SSRF block list: private, loopback, link-local, multicast, CGNAT, docs, reserved.
fn is_blocked_ipv4_address(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || is_shared_ipv4_address(ip)
        || is_benchmark_ipv4_address(ip)
        || is_reserved_ipv4_address(ip)
}

/// IPv6 SSRF block list: loopback, ULA, link-local, multicast, documentation.
fn is_blocked_ipv6_address(ip: Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || is_unique_local_ipv6(ip)
        || is_unicast_link_local_ipv6(ip)
        || is_documentation_ipv6(ip)
}

/// Unique local addresses (`fc00::/7`).
fn is_unique_local_ipv6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

/// Link-local unicast (`fe80::/10`).
fn is_unicast_link_local_ipv6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// Documentation prefix (`2001:db8::/32`).
fn is_documentation_ipv6(ip: Ipv6Addr) -> bool {
    ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8
}

/// Shared address space / CGNAT (`100.64.0.0/10`).
fn is_shared_ipv4_address(ip: Ipv4Addr) -> bool {
    let [first, second, _, _] = ip.octets();
    first == 100 && (second & 0xc0) == 64
}

/// Benchmarking range (`198.18.0.0/15`).
fn is_benchmark_ipv4_address(ip: Ipv4Addr) -> bool {
    let [first, second, _, _] = ip.octets();
    first == 198 && (second == 18 || second == 19)
}

/// Class E / reserved (`240.0.0.0/4`).
fn is_reserved_ipv4_address(ip: Ipv4Addr) -> bool {
    ip.octets()[0] >= 240
}

/// Reads a response body up to [`MAX_PDF_DOWNLOAD_BYTES`] and verifies PDF magic bytes.
///
/// # Errors
///
/// Returns an error when the body exceeds the size cap, streaming fails, or content is not a PDF.
async fn read_limited_pdf_response(link: &str, mut response: reqwest::Response) -> Result<Vec<u8>> {
    if let Some(content_length) = response.content_length() {
        if content_length > MAX_PDF_DOWNLOAD_BYTES {
            bail!(
                "Downloaded PDF from {link} is larger than the {} MiB limit.",
                MAX_PDF_DOWNLOAD_BYTES / 1024 / 1024
            );
        }
    }

    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .and_then(|length| length.try_into().ok())
            .unwrap_or_default(),
    );
    while let Some(chunk) = response.chunk().await? {
        let next_len = (bytes.len() as u64).saturating_add(chunk.len() as u64);
        if next_len > MAX_PDF_DOWNLOAD_BYTES {
            bail!(
                "Downloaded PDF from {link} is larger than the {} MiB limit.",
                MAX_PDF_DOWNLOAD_BYTES / 1024 / 1024
            );
        }
        bytes.extend_from_slice(&chunk);
    }

    ensure_pdf_response(link, &bytes)?;
    Ok(bytes)
}

/// Ensures downloaded bytes look like a PDF (`%PDF-` within the first 1 KiB).
///
/// # Errors
///
/// Returns an error with a short response preview when the magic bytes are absent.
pub(crate) fn ensure_pdf_response(link: &str, bytes: &[u8]) -> Result<()> {
    if bytes
        .windows(b"%PDF-".len())
        .take(1024)
        .any(|window| window == b"%PDF-")
    {
        return Ok(());
    }

    let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(160)])
        .replace(['\n', '\r'], " ");
    bail!("Downloaded content from {link} was not a PDF. Response preview: {preview}");
}

/// Wire wrapper for `GET /user` (`{ "user": … }`).
#[derive(Debug, Deserialize)]
struct UserResponse {
    /// Nested authenticated user profile.
    user: RaindropUser,
}

/// Authenticated Raindrop user from `GET /user` (`user` object).
#[derive(Debug, Deserialize)]
pub(crate) struct RaindropUser {
    /// Raindrop account id JSON `_id` (used as import-source identity).
    #[serde(rename = "_id", deserialize_with = "i64_from_json")]
    pub(crate) id: i64,
    /// Display name JSON `fullName` for the import preview account label.
    #[serde(rename = "fullName")]
    pub(crate) full_name: Option<String>,
}

/// Wire wrapper for collection list endpoints (`{ "items": […] }`).
#[derive(Debug, Deserialize)]
struct CollectionsResponse {
    /// Collections returned on this page/endpoint.
    items: Vec<RaindropCollection>,
}

/// Raindrop collection (folder) from `/collections` and `/collections/childrens`.
///
/// Mirrored into local library folders when the user preserves Raindrop structure.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RaindropCollection {
    /// Collection id JSON `_id` (stable remote key for folder mapping).
    #[serde(rename = "_id", deserialize_with = "i64_from_json")]
    pub(crate) id: i64,
    /// User-visible collection name JSON `title`.
    #[serde(default)]
    pub(crate) title: Option<String>,
    /// Raindrop sort key JSON `sort` (sibling order under the parent).
    #[serde(default)]
    #[serde(deserialize_with = "i64_or_default")]
    pub(crate) sort: i64,
    /// Optional parent collection JSON `parent` (`$id` ref) for nested trees.
    #[serde(default, deserialize_with = "optional_ref")]
    pub(crate) parent: Option<RaindropRef>,
}

impl RaindropCollection {
    /// Display title, or a stable fallback including the collection id.
    pub(crate) fn title(&self) -> String {
        self.title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Raindrop collection {}", self.id))
    }

    /// Parent collection id when nested.
    pub(crate) fn parent_id(&self) -> Option<i64> {
        self.parent.as_ref().map(|parent| parent.id)
    }
}

/// Wire wrapper for paged raindrop list responses (`{ "items": […] }`).
#[derive(Debug, Deserialize)]
struct RaindropsResponse {
    /// Raindrops on this page.
    items: Vec<Raindrop>,
}

/// Wire wrapper for a single raindrop fetch (`{ "item": … }`).
#[derive(Debug, Deserialize)]
struct RaindropResponse {
    /// The requested raindrop.
    item: Raindrop,
}

/// One Raindrop bookmark/item from the raindrops APIs (wire JSON + import helpers).
///
/// Fields map 1:1 onto Raindrop’s public JSON; helpers (`is_pdf`, `download_link`, …)
/// normalize them for PDF-Folio import without exposing the raw DTO to the UI.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Raindrop {
    /// Item id JSON `_id` (remote key for public import candidates).
    #[serde(rename = "_id", deserialize_with = "i64_from_json")]
    pub(crate) id: i64,
    /// Bookmark title JSON `title` (preferred display label).
    #[serde(default)]
    pub(crate) title: Option<String>,
    /// Primary bookmark URL JSON `link` (fallback download target for external PDFs).
    #[serde(default)]
    pub(crate) link: String,
    /// Cover image URL JSON `cover` (thumbnail preference over `media`).
    #[serde(default)]
    pub(crate) cover: Option<String>,
    /// Media array JSON `media` used as thumbnail fallback when `cover` is empty.
    #[serde(default)]
    pub(crate) media: Vec<RaindropMedia>,
    /// Raindrop item type JSON `type` (e.g. `"document"`, `"link"`) for PDF heuristics.
    #[serde(rename = "type")]
    pub(crate) item_type: Option<String>,
    /// Owning collection JSON `collection` (`$id` ref).
    #[serde(default, deserialize_with = "optional_ref")]
    pub(crate) collection: Option<RaindropRef>,
    /// Tag strings JSON `tags` copied onto the local entry after import.
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    /// Attached file object JSON `file` when Raindrop hosts the PDF.
    pub(crate) file: Option<RaindropFile>,
    /// Last remote update ISO timestamp JSON `lastUpdate`.
    #[serde(rename = "lastUpdate")]
    pub(crate) last_update: Option<String>,
    /// Local-only: prefer authenticated `/raindrop/{id}/cache` download path.
    ///
    /// Not present on the wire; set when reconstructing from a candidate or when
    /// file metadata indicates a Raindrop-hosted upload.
    #[serde(skip)]
    pub(crate) uploaded_file: bool,
}

impl Raindrop {
    /// Heuristic: attached PDF file, document type with `.pdf` link, or link ends with `.pdf`.
    pub(crate) fn is_pdf(&self) -> bool {
        self.file.as_ref().is_some_and(|file| file.is_pdf())
            || self
                .item_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("document"))
                && self.link.to_lowercase().contains(".pdf")
            || self.link.to_lowercase().ends_with(".pdf")
    }

    /// Collection id when assigned.
    pub(crate) fn collection_id(&self) -> Option<i64> {
        self.collection.as_ref().map(|collection| collection.id)
    }

    /// Preferred file name (file metadata, else title).
    pub(crate) fn file_name(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|file| file.name.clone())
            .or_else(|| self.title.clone())
    }

    /// Declared remote file size when present.
    pub(crate) fn file_size(&self) -> Option<u64> {
        self.file.as_ref().and_then(|file| file.size)
    }

    /// Best download URL (file link if set, otherwise bookmark `link`).
    pub(crate) fn download_link(&self) -> &str {
        self.file
            .as_ref()
            .and_then(|file| file.link.as_deref())
            .filter(|link| !link.trim().is_empty())
            .unwrap_or(&self.link)
    }

    /// True when this item should use Raindrop’s authenticated cache download path.
    pub(crate) fn has_uploaded_file(&self) -> bool {
        if self.uploaded_file {
            return true;
        }
        self.file.as_ref().is_some_and(|file| {
            file.is_pdf()
                || file
                    .link
                    .as_deref()
                    .is_some_and(download_requires_raindrop_auth)
        })
    }

    /// Cover image or first media link suitable as a thumbnail.
    pub(crate) fn thumbnail_url(&self) -> Option<String> {
        self.cover
            .as_ref()
            .filter(|cover| !cover.trim().is_empty())
            .cloned()
            .or_else(|| {
                self.media
                    .iter()
                    .find_map(|media| media.link.as_ref().filter(|link| !link.trim().is_empty()))
                    .cloned()
            })
    }

    /// User-visible label (title, file name, or `Raindrop {id}`).
    pub(crate) fn display_label(&self) -> String {
        self.title
            .clone()
            .or_else(|| self.file_name())
            .unwrap_or_else(|| format!("Raindrop {}", self.id))
    }

    /// Builds a public [`RaindropPdfCandidate`] for import UI/preview.
    pub(crate) fn to_candidate(
        &self,
        collection_titles: &HashMap<i64, String>,
    ) -> RaindropPdfCandidate {
        let collection_id = self.collection_id();
        RaindropPdfCandidate {
            id: self.id,
            title: self.display_label(),
            file_name: self.file_name(),
            file_size: self.file_size(),
            thumbnail_url: self.thumbnail_url(),
            download_link: self.download_link().to_owned(),
            uploaded_file: self.has_uploaded_file(),
            tags: self.tags.clone(),
            collection_id,
            collection_title: collection_id.and_then(|id| collection_titles.get(&id).cloned()),
            remote_updated_at: self.last_update.clone(),
        }
    }
}

/// Raindrop `$id` object reference (collection parent / item `collection`).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RaindropRef {
    /// Referenced entity id JSON `$id` (Raindrop’s nested-id convention).
    #[serde(rename = "$id", deserialize_with = "i64_from_json")]
    pub(crate) id: i64,
}

/// Attached file metadata JSON `file` on a raindrop (uploads and some documents).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RaindropFile {
    /// Original filename JSON `name` (preferred local save stem when present).
    pub(crate) name: Option<String>,
    /// Direct file URL JSON `link` (preferred over the bookmark `link` for downloads).
    #[serde(default)]
    pub(crate) link: Option<String>,
    /// Declared size in bytes JSON `size` (may be string or number on the wire).
    #[serde(default, deserialize_with = "optional_u64")]
    pub(crate) size: Option<u64>,
    /// MIME type JSON `type` (e.g. `application/pdf`) for [`RaindropFile::is_pdf`].
    #[serde(rename = "type")]
    pub(crate) mime_type: Option<String>,
}

/// Media entry from JSON `media[]` used only for cover/thumbnail fallbacks.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RaindropMedia {
    /// Image/URL string JSON `link` when Raindrop embeds preview media.
    #[serde(default)]
    link: Option<String>,
}

/// Deserializes optional `u64` from a JSON number or numeric string.
fn optional_u64<'de, D>(deserializer: D) -> std::result::Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Number(number)) => number.as_u64(),
        Some(serde_json::Value::String(value)) => value.parse::<u64>().ok(),
        _ => None,
    })
}

/// Deserializes optional Raindrop `$id` refs from object, number, or string forms.
fn optional_ref<'de, D>(deserializer: D) -> std::result::Result<Option<RaindropRef>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    let id = match value {
        serde_json::Value::Object(mut object) => object.remove("$id"),
        value @ (serde_json::Value::Number(_) | serde_json::Value::String(_)) => Some(value),
        _ => None,
    };
    match id {
        Some(serde_json::Value::Number(number)) => Ok(number.as_i64().map(|id| RaindropRef { id })),
        Some(serde_json::Value::String(value)) => {
            Ok(value.parse::<i64>().ok().map(|id| RaindropRef { id }))
        }
        _ => Ok(None),
    }
}

/// Deserializes a required `i64` id from a JSON number or numeric string.
fn i64_from_json<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .ok_or_else(|| serde::de::Error::custom("id was not a signed integer")),
        serde_json::Value::String(value) => value.parse::<i64>().map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("id was not a number or string")),
    }
}

/// Deserializes optional `i64` (number/string), defaulting to `0` when absent or invalid.
fn i64_or_default<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Number(number)) => number.as_i64().unwrap_or_default(),
        Some(serde_json::Value::String(value)) => value.parse::<i64>().unwrap_or_default(),
        _ => 0,
    })
}

impl RaindropFile {
    /// True when MIME is `application/pdf` or the name ends with `.pdf`.
    pub(crate) fn is_pdf(&self) -> bool {
        self.mime_type
            .as_deref()
            .is_some_and(|mime| mime.eq_ignore_ascii_case("application/pdf"))
            || self
                .name
                .as_deref()
                .is_some_and(|name| name.to_lowercase().ends_with(".pdf"))
    }
}
