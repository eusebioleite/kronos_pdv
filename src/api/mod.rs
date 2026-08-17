use anyhow::{Context, Result, anyhow};
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub token_type: String,
}

#[derive(Debug, Serialize)]
pub struct ActivityCustomField {
    #[serde(rename = "customFieldId")]
    pub custom_field_id: i64,
    pub value: Value,
}

#[derive(Debug, Serialize)]
pub struct ActivityComplete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    #[serde(rename = "processId")]
    pub process_id: i64,
    pub title: String,
    pub detail: String,
    #[serde(rename = "plannedDate")]
    pub planned_date: String,
    #[serde(rename = "functionalRequirements")]
    pub functional_requirements: String,
    #[serde(rename = "requesterId")]
    pub requester_id: i64,
    #[serde(rename = "sellerId")]
    pub seller_id: i64,
    #[serde(rename = "companyId")]
    pub company_id: i64,
    #[serde(rename = "departmentId")]
    pub department_id: Option<i64>,
    #[serde(rename = "customFields")]
    pub custom_fields: Vec<ActivityCustomField>,
}

pub async fn get_token(config: &Config) -> Result<String> {
    let client = Client::new();
    let url = format!("{}/realms/dealercrm/protocol/openid-connect/token", config.auth_url);

    let body = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}",
        urlencoding::encode(&config.client_id),
        urlencoding::encode(&config.client_secret)
    );

    let res = client
        .post(&url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .context("Failed to send token request")?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!("Failed to get token: HTTP {} - {}", status, body));
    }

    let token_res: TokenResponse = res.json().await.context("Failed to parse token response")?;
    Ok(token_res.access_token)
}

pub async fn new_card(config: &Config, token: &str, card: &ActivityComplete) -> Result<()> {
    let client = Client::new();
    let url = format!("{}/v3/works/core/activities", config.api_url);

    let res = client
        .post(&url)
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .header("ContextGuid", &config.context_guid)
        .json(card)
        .send()
        .await
        .context("Failed to send new_card request")?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!("Failed to create card: HTTP {} - {}", status, body));
    }

    Ok(())
}

pub async fn update_card(config: &Config, token: &str, code: i64, card: &ActivityComplete) -> Result<()> {
    let client = Client::new();
    let url = format!("{}/v3/works/core/activities/{}", config.api_url, code);

    let res = client
        .patch(&url)
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .header("ContextGuid", &config.context_guid)
        .json(card)
        .send()
        .await
        .context("Failed to send update_card request")?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!("Failed to update card {}: HTTP {} - {}", code, status, body));
    }

    Ok(())
}

pub async fn delete_card(config: &Config, token: &str, code: i64) -> Result<()> {
    let client = Client::new();
    let url = format!("{}/v3/works/core/activities/{}", config.api_url, code);

    let res = client
        .delete(&url)
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .header("ContextGuid", &config.context_guid)
        .send()
        .await
        .context("Failed to send delete_card request")?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!("Failed to delete card {}: HTTP {} - {}", code, status, body));
    }

    Ok(())
}
