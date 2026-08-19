use anyhow::{Context, Result, anyhow};
use serde::Serialize;

pub mod auth;

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_stages_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_to: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requester_person_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responsible_person_code: Option<i64>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    /// Only set when updating an existing card (PATCH). Omitted on POST.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    pub title: String,
    pub detail: String,
    pub type_activity_code: i64,
    pub planned_date: chrono::NaiveDate,
    pub replanned_date: chrono::NaiveDate,
    pub objective: String,
    pub script: String,
    pub functional_requirements: String,
    pub business_rule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_stages: Option<Vec<WorkflowStage>>,
}

pub async fn new_card(card: &Activity) -> Result<()> {
    let config = &crate::config::get().config;
    let client = auth::get_client();

    let url = format!("{}/v3/works/core/activities", config.api_url);
    let body_bytes = serde_json::to_vec(card)
        .context("Failed to serialize Activity card to JSON payload")?;

    let res = client
        .post(&url)
        .header("ContextGuid", &config.context_guid)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body_bytes)
        .send()
        .await
        .with_context(|| format!("Failed to send new_card HTTP request to '{}'", url))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Failed to create card via API at '{}': HTTP {} - {}",
            url,
            status,
            body
        ));
    }

    Ok(())
}

pub async fn update_card(guid: &str, card: &Activity) -> Result<()> {
    let config = &crate::config::get().config;
    let client = auth::get_client();

    // The PATCH endpoint is /activities (no {Guid} in path).
    // The guid identifying which card to update must be in the request body.
    let url = format!("{}/v3/works/core/activities", config.api_url);

    let mut card_with_guid = card.clone();
    card_with_guid.guid = Some(guid.to_string());
    card_with_guid.workflow_stages = None; // Never send workflow stages on PATCH to prevent stage duplication

    let body_bytes = serde_json::to_vec(&card_with_guid).with_context(|| {
        format!(
            "Failed to serialize Activity card with guid {} to JSON payload",
            guid
        )
    })?;

    let res = client
        .patch(&url)
        .header("ContextGuid", &config.context_guid)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body_bytes)
        .send()
        .await
        .with_context(|| format!("Failed to send update_card HTTP request to '{}'", url))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Failed to update card with guid {} via API at '{}': HTTP {} - {}",
            guid,
            url,
            status,
            body
        ));
    }

    Ok(())
}

pub async fn delete_card(guid: &str) -> Result<()> {
    let config = &crate::config::get().config;
    let client = auth::get_client();

    let url = format!("{}/v3/works/core/activities/{}", config.api_url, guid);

    let res = client
        .delete(&url)
        .header("ContextGuid", &config.context_guid)
        .send()
        .await
        .with_context(|| format!("Failed to send delete_card HTTP request to '{}'", url))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();

        // If the card is already deleted or not found, treat as idempotent success
        if (status == reqwest::StatusCode::BAD_REQUEST
            && (body.contains("excluído") || body.contains("excluido")))
            || status == reqwest::StatusCode::NOT_FOUND
        {
            tracing::warn!(
                "Card with guid {} was already deleted or not found (HTTP {} - {})",
                guid,
                status,
                body.trim()
            );
            return Ok(());
        }

        return Err(anyhow!(
            "Failed to delete card with guid {} via API at '{}': HTTP {} - {}",
            guid,
            url,
            status,
            body
        ));
    }

    Ok(())
}
