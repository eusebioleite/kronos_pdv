use anyhow::{Context, Result};
use reqwest::{Client, Request, Response, header};
use reqwest_middleware::{Middleware, Next};
use serde::Deserialize;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;
use chrono::{DateTime, Duration, Utc};

use crate::config::Config;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub token_type: String,
}

#[derive(Debug)]
pub struct TokenState {
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
}

pub struct AuthManager {
    config: Arc<Config>,
    client: Client,
}

impl AuthManager {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    pub async fn get_valid_token(&self) -> Result<String> {
        let state_lock = Self::global_state();

        // 1. Check if we have a valid token (read lock)
        {
            let state = state_lock.read().await;
            if let Some(token) = state.as_ref() {
                // Add a small buffer (30 seconds) to avoid edge cases
                if token.expires_at > Utc::now() + Duration::try_seconds(30).unwrap_or_default() {
                    return Ok(token.access_token.clone());
                }
            }
        } // drop read lock

        // 2. Need to refresh or get new token. Get write lock.
        let mut state = state_lock.write().await;
        // Double-check in case another task refreshed it while we were waiting
        if let Some(token) = state.as_ref()
            && token.expires_at > Utc::now() + Duration::try_seconds(30).unwrap_or_default()
        {
            return Ok(token.access_token.clone());
        }

        // 3. Fetch new token
        let url = self.config.auth_url.clone();
        
        let body = format!(
            "grant_type=client_credentials&client_id={}&client_secret={}",
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.client_secret)
        );

        let res = self.client
            .post(&url)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .with_context(|| format!("Failed to send token request to '{}'", url))?;

        if !res.status().is_success() {
            let status = res.status();
            let body_text = res.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Failed to get token from '{}': HTTP {} - {}", url, status, body_text));
        }

        let token_res: TokenResponse = res
            .json()
            .await
            .with_context(|| format!("Failed to parse token JSON response from '{}'", url))?;
        
        let access_token = token_res.access_token.clone();
        *state = Some(TokenState {
            access_token: access_token.clone(),
            expires_at: Utc::now() + Duration::try_seconds(token_res.expires_in as i64).unwrap_or(Duration::zero()),
        });

        Ok(access_token)
    }

    fn global_state() -> &'static RwLock<Option<TokenState>> {
        static TOKEN_STATE: OnceLock<RwLock<Option<TokenState>>> = OnceLock::new();
        TOKEN_STATE.get_or_init(|| RwLock::new(None))
    }
}

pub struct AuthMiddleware {
    pub manager: AuthManager,
}

#[async_trait::async_trait]
impl Middleware for AuthMiddleware {
    async fn handle(
        &self,
        mut req: Request,
        extensions: &mut http::Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        // Skip adding token if request is to auth_url
        if req.url().as_str().starts_with(&self.manager.config.auth_url) {
            return next.run(req, extensions).await;
        }

        let token = self
            .manager
            .get_valid_token()
            .await
            .context("Failed to retrieve valid auth token for request")
            .map_err(reqwest_middleware::Error::Middleware)?;

        let header_value = header::HeaderValue::from_str(&format!("Bearer {}", token))
            .with_context(|| "Failed to create Bearer authorization header value")
            .map_err(reqwest_middleware::Error::Middleware)?;
        
        req.headers_mut().insert(header::AUTHORIZATION, header_value);

        next.run(req, extensions).await
    }
}

static API_CLIENT: OnceLock<reqwest_middleware::ClientWithMiddleware> = OnceLock::new();

/// Returns a reference to the global authenticated HTTP client.
/// Panics if called before `api::auth::init()`.
pub fn get_client() -> &'static reqwest_middleware::ClientWithMiddleware {
    API_CLIENT.get().expect("api::auth::init() must be called before get_client()")
}

/// Initializes the global API client. Must be called once at startup,
/// after `config::init()`.
pub fn init() {
    let client = build_api_client();
    API_CLIENT.set(client).ok(); // Silently ignore if called twice
}

fn build_api_client() -> reqwest_middleware::ClientWithMiddleware {
    let reqwest_client = Client::builder()
        .build()
        .expect("Failed to build reqwest client");

    let auth_manager = AuthManager::new(Arc::new(crate::config::get().config.clone()));

    reqwest_middleware::ClientBuilder::new(reqwest_client)
        .with(AuthMiddleware { manager: auth_manager })
        .build()
}
