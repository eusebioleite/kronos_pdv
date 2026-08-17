use anyhow::{Context, Result, anyhow};
use reqwest_middleware::ClientWithMiddleware;
use serde::Serialize;
use serde_json::Value;

use crate::config::Config;

pub mod auth;

#[derive(Debug, Serialize)]
pub struct ActivityComplete {
    pub title: String,
    pub detail: String,
    pub code: i64,
    pub guid: String,
    pub type_activity_code: String,
    pub planned_date: chrono::NaiveDate,
    pub replanned_date: chrono::NaiveDate,
    pub objective: String,
    pub script: String,
    pub functional_requirements: String,
    pub workflow_stages: Vec<WorkflowStages>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowStages {
    workflow_stages_code: i64,
    pub code: i64,
    pub guid: String,
    pub order: i64,
    pub order_to: i64,
    pub requester_person_code: i64,
    pub responsible_person_code: i64,
}

impl ActivityComplete {
    pub fn from_order(order: Order) -> Self {
        ActivityComplete {
            title: format!("Pedido {} - {}", order.order_code, order.order_kind),
            detail: format!("Cliente: {} - {}", order.customer_code, order.customer_name),
            code: order.order_code,
            guid: order.order_code,
            type_activity_code: order.order_code,
            planned_date: order.order_date,
            replanned_date: order.order_date,
            objective: order.order_code,
            script: order.order_code,
            functional_requirements: order.order_code,
            workflow_stages: vec![
                WorkflowStages {
                    workflow_stages_code: order.,
                    code: order.order_code,
                    guid: order.order_code,
                    order: order.order_code,
                    order_to: order.order_code,
                    requester_person_code: order.order_code,
                    responsible_person_code: order.order_code,
                }
            ],
        }
    }
}

pub async fn new_card(
    client: &ClientWithMiddleware,
    config: &Config,
    card: &ActivityComplete,
) -> Result<()> {
    let url = format!("{}/v3/works/core/activities", config.api_url);
    let body_bytes = serde_json::to_vec(card)
        .context("Failed to serialize ActivityComplete card to JSON payload")?;

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

pub async fn update_card(
    client: &ClientWithMiddleware,
    config: &Config,
    code: i64,
    card: &ActivityComplete,
) -> Result<()> {
    let url = format!("{}/v3/works/core/activities/{}", config.api_url, code);
    let body_bytes = serde_json::to_vec(card).with_context(|| {
        format!(
            "Failed to serialize ActivityComplete card {} to JSON payload",
            code
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
            "Failed to update card {} via API at '{}': HTTP {} - {}",
            code,
            url,
            status,
            body
        ));
    }

    Ok(())
}

pub async fn delete_card(client: &ClientWithMiddleware, config: &Config, code: i64) -> Result<()> {
    let url = format!("{}/v3/works/core/activities/{}", config.api_url, code);

    let res = client
        .delete(&url)
        .header("ContextGuid", &config.context_guid)
        .send()
        .await
        .with_context(|| format!("Failed to send delete_card HTTP request to '{}'", url))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Failed to delete card {} via API at '{}': HTTP {} - {}",
            code,
            url,
            status,
            body
        ));
    }

    Ok(())
}
