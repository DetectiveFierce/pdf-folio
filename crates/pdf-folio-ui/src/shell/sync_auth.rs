use std::path::Path;

use crate::*;
use anyhow::Context;
#[cfg(not(test))]
use pdf_folio_cloud::sync::cached_session;
use pdf_folio_cloud::sync::{sign_in_with_google, GoogleAuthConfig, Session};

const DEFAULT_ALLOWED_GOOGLE_EMAIL: &str = "aidanjwagner03@gmail.com";
const DEFAULT_SYNC_SERVER_BASE_URL: &str = "http://mind-palace:53148";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncAuthRuntime {
    pub state: SyncAuthState,
    pub expected_email: String,
    pub server_base_url: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAuthState {
    SignedOut,
    SigningIn,
    SignedIn { email: String, expires_at: String },
    WrongAccount { email: Option<String> },
}

impl SyncAuthRuntime {
    pub fn load() -> Self {
        let expected_email = expected_google_email();
        let server_base_url = sync_server_base_url();
        #[cfg(test)]
        return Self::signed_in_for_tests(expected_email, server_base_url);

        #[cfg(not(test))]
        match cached_session() {
            Ok(session) if session_matches_expected_email(&session, &expected_email) => Self {
                state: SyncAuthState::SignedIn {
                    email: expected_email.clone(),
                    expires_at: session.expires_at.to_rfc3339(),
                },
                expected_email,
                server_base_url,
                error: None,
            },
            Ok(session) if session.is_valid() => Self {
                state: SyncAuthState::WrongAccount {
                    email: session.email.clone(),
                },
                expected_email,
                server_base_url,
                error: Some(String::from(
                    "That cached Google session is not allowed for this library.",
                )),
            },
            _ => Self {
                state: SyncAuthState::SignedOut,
                expected_email,
                server_base_url,
                error: None,
            },
        }
    }

    #[cfg(test)]
    fn signed_in_for_tests(expected_email: String, server_base_url: String) -> Self {
        Self {
            state: SyncAuthState::SignedIn {
                email: expected_email.clone(),
                expires_at: String::from("test"),
            },
            expected_email,
            server_base_url,
            error: None,
        }
    }

    pub fn is_signed_in(&self) -> bool {
        matches!(self.state, SyncAuthState::SignedIn { .. })
    }

    pub fn apply_signed_in_session(&mut self, session: Session) -> Result<()> {
        if !session_matches_expected_email(&session, &self.expected_email) {
            self.state = SyncAuthState::WrongAccount {
                email: session.email.clone(),
            };
            anyhow::bail!(
                "Signed in as {}, but PDF-Folio is locked to {}.",
                session
                    .email
                    .as_deref()
                    .unwrap_or(session.google_sub.as_str()),
                self.expected_email
            );
        }

        self.state = SyncAuthState::SignedIn {
            email: session
                .email
                .clone()
                .unwrap_or_else(|| self.expected_email.clone()),
            expires_at: session.expires_at.to_rfc3339(),
        };
        self.error = None;
        Ok(())
    }
}

pub(crate) fn sync_sign_in_task(expected_email: String, server_base_url: String) -> Task<Message> {
    Task::perform(
        async move {
            let client_id = load_google_client_id_from_secrets()
                .context("Provide PDF_FOLIO_GOOGLE_CLIENT_ID or a Google client_secret JSON.")?;
            let session = sign_in_with_google(&GoogleAuthConfig {
                client_id,
                sync_server_base_url: server_base_url,
            })
            .await?;
            if !session_matches_expected_email(&session, &expected_email) {
                anyhow::bail!(
                    "Signed in as {}, but PDF-Folio is locked to {}.",
                    session
                        .email
                        .as_deref()
                        .unwrap_or(session.google_sub.as_str()),
                    expected_email
                );
            }
            Ok::<_, anyhow::Error>(session)
        },
        |result| match result {
            Ok(session) => Message::SyncSignInFinished(Ok(session)),
            Err(error) => Message::SyncSignInFinished(Err(error.to_string())),
        },
    )
}

fn expected_google_email() -> String {
    std::env::var("PDF_FOLIO_ALLOWED_GOOGLE_EMAIL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_ALLOWED_GOOGLE_EMAIL.to_owned())
}

fn sync_server_base_url() -> String {
    std::env::var("PDF_FOLIO_SYNC_SERVER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SYNC_SERVER_BASE_URL.to_owned())
}

fn load_google_client_id_from_secrets() -> Option<String> {
    std::env::var("PDF_FOLIO_GOOGLE_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            let secrets_dir = Path::new("secrets");
            let path = std::fs::read_dir(secrets_dir)
                .ok()?
                .filter_map(std::result::Result::ok)
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with("client_secret_") && name.ends_with(".json")
                        })
                })?;
            let json = std::fs::read_to_string(path).ok()?;
            let value = serde_json::from_str::<serde_json::Value>(&json).ok()?;
            value
                .get("installed")
                .and_then(|installed| installed.get("client_id"))
                .and_then(|client_id| client_id.as_str())
                .map(str::to_owned)
        })
}

fn session_matches_expected_email(session: &Session, expected_email: &str) -> bool {
    session.is_valid()
        && session
            .email
            .as_deref()
            .is_some_and(|email| email.eq_ignore_ascii_case(expected_email))
}
