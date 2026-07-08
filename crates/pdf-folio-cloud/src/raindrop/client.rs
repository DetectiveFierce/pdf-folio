//! Raindrop.io HTTP client and remote response model.

use std::collections::HashMap;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::header::AUTHORIZATION;
use serde::Deserialize;
use url::Url;

use super::{
    RaindropPdfCandidate, API_BASE, MAX_PER_PAGE, ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS,
    ZIP_PREPARING_PROGRESS_BASIS_POINTS,
};

impl RaindropPdfCandidate {
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

pub(crate) struct RaindropClient {
    http: reqwest::Client,
    token: String,
}

impl RaindropClient {
    pub(crate) fn new(token: String) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent("PDF-Folio Raindrop Import")
                .build()?,
            token,
        })
    }

    pub(crate) async fn user(&self) -> Result<RaindropUser> {
        let response = self
            .get_json::<UserResponse>(&format!("{API_BASE}/user"))
            .await?;
        Ok(response.user)
    }

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

    pub(crate) async fn raindrop(&self, id: i64) -> Result<Raindrop> {
        let response = self
            .get_json::<RaindropResponse>(&format!("{API_BASE}/raindrop/{id}"))
            .await?;
        Ok(response.item)
    }

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

    pub(crate) async fn download_pdf(&self, link: &str) -> Result<Vec<u8>> {
        let mut request = self.http.get(link).header("Accept", "application/pdf,*/*");
        if download_requires_raindrop_auth(link) {
            request = request.header(AUTHORIZATION, format!("Bearer {}", self.token));
        }
        let response = request.send().await?.error_for_status()?;
        let bytes = response.bytes().await?.to_vec();
        ensure_pdf_response(link, &bytes)?;
        Ok(bytes)
    }

    pub(crate) async fn download_pdf_for_raindrop(&self, raindrop: &Raindrop) -> Result<Vec<u8>> {
        if raindrop.has_uploaded_file() {
            return self
                .download_pdf(&format!("{API_BASE}/raindrop/{}/cache", raindrop.id))
                .await;
        }

        self.download_pdf(raindrop.download_link()).await
    }

    pub(crate) async fn download_pdf_export_zip(&self, mut progress: impl FnMut(u16)) -> Result<Vec<u8>> {
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
                .replace('\n', " ")
                .replace('\r', " ");
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

fn download_requires_raindrop_auth(link: &str) -> bool {
    Url::parse(link)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "raindrop.io" || host.ends_with(".raindrop.io"))
}

pub(crate) fn ensure_pdf_response(link: &str, bytes: &[u8]) -> Result<()> {
    if bytes
        .windows(b"%PDF-".len())
        .take(1024)
        .any(|window| window == b"%PDF-")
    {
        return Ok(());
    }

    let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(160)])
        .replace('\n', " ")
        .replace('\r', " ");
    bail!("Downloaded content from {link} was not a PDF. Response preview: {preview}");
}

#[derive(Debug, Deserialize)]
struct UserResponse {
    user: RaindropUser,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RaindropUser {
    #[serde(rename = "_id", deserialize_with = "i64_from_json")]
    pub(crate) id: i64,
    #[serde(rename = "fullName")]
    pub(crate) full_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CollectionsResponse {
    items: Vec<RaindropCollection>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RaindropCollection {
    #[serde(rename = "_id", deserialize_with = "i64_from_json")]
    pub(crate) id: i64,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "i64_or_default")]
    pub(crate) sort: i64,
    #[serde(default, deserialize_with = "optional_ref")]
    pub(crate) parent: Option<RaindropRef>,
}

impl RaindropCollection {
    pub(crate) fn title(&self) -> String {
        self.title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Raindrop collection {}", self.id))
    }

    pub(crate) fn parent_id(&self) -> Option<i64> {
        self.parent.as_ref().map(|parent| parent.id)
    }
}

#[derive(Debug, Deserialize)]
struct RaindropsResponse {
    items: Vec<Raindrop>,
}

#[derive(Debug, Deserialize)]
struct RaindropResponse {
    item: Raindrop,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Raindrop {
    #[serde(rename = "_id", deserialize_with = "i64_from_json")]
    pub(crate) id: i64,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) link: String,
    #[serde(default)]
    pub(crate) cover: Option<String>,
    #[serde(default)]
    pub(crate) media: Vec<RaindropMedia>,
    #[serde(rename = "type")]
    pub(crate) item_type: Option<String>,
    #[serde(default, deserialize_with = "optional_ref")]
    pub(crate) collection: Option<RaindropRef>,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    pub(crate) file: Option<RaindropFile>,
    #[serde(rename = "lastUpdate")]
    pub(crate) last_update: Option<String>,
    #[serde(skip)]
    pub(crate) uploaded_file: bool,
}

impl Raindrop {
    pub(crate) fn is_pdf(&self) -> bool {
        self.file.as_ref().is_some_and(|file| file.is_pdf())
            || self
                .item_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("document"))
                && self.link.to_lowercase().contains(".pdf")
            || self.link.to_lowercase().ends_with(".pdf")
    }

    pub(crate) fn collection_id(&self) -> Option<i64> {
        self.collection.as_ref().map(|collection| collection.id)
    }

    pub(crate) fn file_name(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|file| file.name.clone())
            .or_else(|| self.title.clone())
    }

    pub(crate) fn file_size(&self) -> Option<u64> {
        self.file.as_ref().and_then(|file| file.size)
    }

    pub(crate) fn download_link(&self) -> &str {
        self.file
            .as_ref()
            .and_then(|file| file.link.as_deref())
            .filter(|link| !link.trim().is_empty())
            .unwrap_or(&self.link)
    }

    pub(crate) fn has_uploaded_file(&self) -> bool {
        if self.uploaded_file {
            return true;
        }
        self.file.as_ref().is_some_and(|file| {
            file.is_pdf()
                || file
                    .link
                    .as_deref()
                    .is_some_and(|link| download_requires_raindrop_auth(link))
        })
    }

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

    pub(crate) fn display_label(&self) -> String {
        self.title
            .clone()
            .or_else(|| self.file_name())
            .unwrap_or_else(|| format!("Raindrop {}", self.id))
    }

    pub(crate) fn to_candidate(&self, collection_titles: &HashMap<i64, String>) -> RaindropPdfCandidate {
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

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RaindropRef {
    #[serde(rename = "$id", deserialize_with = "i64_from_json")]
    pub(crate) id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RaindropFile {
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) link: Option<String>,
    #[serde(default, deserialize_with = "optional_u64")]
    pub(crate) size: Option<u64>,
    #[serde(rename = "type")]
    pub(crate) mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RaindropMedia {
    #[serde(default)]
    link: Option<String>,
}

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
