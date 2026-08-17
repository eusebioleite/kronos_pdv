use std::collections::HashMap;
use anyhow::{Context, Result, anyhow};
use sibyl::{Row, Session};
use tracing::{info, error};

use crate::repository::queue::{Queue, ErrorUpdate};
use crate::api::{self, ActivityComplete, ActivityCustomField};
use crate::dealercrm;
use crate::config::Config;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Order {
    pub order_code: String,
    pub order_kind: String,
    pub schedule_code: Option<String>,
    pub item_sequence: Option<i32>,
    pub item_code: Option<String>,
    pub nature_code: Option<String>,
    pub nature_description: Option<String>,
    pub customer_code: Option<String>,
    pub customer_name: Option<String>,
    pub quantity: Option<f64>,
    pub delivery_date: Option<String>,
}

impl Order {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            order_code: row.get::<Option<String>, _>(0usize)?.unwrap_or_default(),
            order_kind: row.get::<Option<String>, _>(1usize)?.unwrap_or_default(),
            schedule_code: row.get(2usize)?,
            item_sequence: row.get(3usize)?,
            item_code: row.get(4usize)?,
            nature_code: row.get(5usize)?,
            nature_description: row.get(6usize)?,
            customer_code: row.get(7usize)?,
            customer_name: row.get(8usize)?,
            quantity: row.get(9usize)?,
            delivery_date: row.get(10usize)?,
        })
    }
}

pub async fn build_cards(session: &Session<'_>, order_code: &str) -> Result<Vec<ActivityComplete>> {
    let sql = "
        SELECT 
            p.ped_codigo,
            p.ped_especie,
            prv.prv_codigo,
            prv.prv_seqite,
            prv.prv_codpro,
            prv.prv_codnat,
            n.nat_descri,
            p.ped_codcli,
            c.cli_razao,
            prv.prv_qtde,
            to_char(prv.prv_dtprog, 'YYYY-MM-DD') AS prv_dtprog
        FROM f_pedvenda p
        JOIN f_prgven prv ON p.ped_codigo = prv.prv_codped
        LEFT JOIN f_cdnat n ON prv.prv_codnat = n.nat_codigo
        LEFT JOIN f_cdcli c ON p.ped_codcli = c.cli_codigo
        WHERE p.ped_codigo = :1
    ";

    let stmt = session.prepare(sql).await.context("Failed to prepare build_cards statement")?;
    let rows = stmt.query(order_code).await.context("Failed to query order details")?;

    let mut orders = Vec::new();
    while let Some(row) = rows.next().await.context("Failed to fetch order row")? {
        orders.push(Order::from_row(&row)?);
    }

    // Config defaults should ideally be passed, but we'll mock them for ActivityComplete
    let mut cards = Vec::new();

    // Grouping logic (PDV-A grouped by date, PDV-F grouped into one)
    let mut grouped: HashMap<String, Vec<Order>> = HashMap::new();
    
    for order in orders {
        let key = if order.order_kind == "PDV-A" {
            order.delivery_date.clone().unwrap_or_else(|| "1970-01-01".to_string())
        } else {
            "ALL".to_string()
        };
        grouped.entry(key).or_default().push(order);
    }

    for (date_key, group) in grouped {
        let first = &group[0];
        let title = format!("Pedido {} - Cliente {}", first.order_code, first.customer_name.as_deref().unwrap_or(""));
        let func_req = if first.order_kind == "PDV-A" {
            format!("ERP-{}-{}", first.order_code, date_key)
        } else {
            format!("ERP-{}-FECHADO", first.order_code)
        };

        let mut detail = String::new();
        for item in &group {
            detail.push_str(&format!("Item: {} Qtd: {}\n", item.item_code.as_deref().unwrap_or(""), item.quantity.unwrap_or(0.0)));
        }

        cards.push(ActivityComplete {
            code: None,
            process_id: 1, // These should come from config
            title,
            detail,
            planned_date: if date_key == "ALL" { "1970-01-01".to_string() } else { date_key },
            functional_requirements: func_req,
            requester_id: 1, // From config
            seller_id: 1, // From config
            company_id: 1, // From config
            department_id: None,
            custom_fields: vec![],
        });
    }

    Ok(cards)
}

pub async fn sync_queue(
    session: &Session<'_>, 
    card: &Queue, 
    config: &Config, 
    mysql_pool: &sqlx::MySqlPool,
) -> Result<()> {
    
    let token = api::get_token(config).await.context("Failed to get API token")?;

    if card.status == "EXCLUIR" {
        delete_cards(card.pedido.clone(), mysql_pool, config, &token).await?;
        crate::repository::queue::update_status(session, "EXCLUIDO", card).await?;
        return Ok(());
    }

    let desired_cards = build_cards(session, &card.pedido).await?;
    sync_cards(&card.pedido, desired_cards, mysql_pool, config, &token).await?;

    crate::repository::queue::update_status(session, "SUCESSO", card).await?;

    Ok(())
}

async fn sync_cards(
    order_code: &str,
    desired_cards: Vec<ActivityComplete>,
    mysql_pool: &sqlx::MySqlPool,
    config: &Config,
    token: &str,
) -> Result<()> {
    let existing_activities = dealercrm::fetch_activities(mysql_pool, order_code).await?;

    // Create a map of existing activities by functional_requirements
    let mut existing_map: HashMap<String, dealercrm::Activity> = HashMap::new();
    for act in existing_activities {
        if let Some(req) = &act.functional_requirements {
            existing_map.insert(req.clone(), act);
        }
    }

    for mut desired in desired_cards {
        if let Some(existing) = existing_map.remove(&desired.functional_requirements) {
            desired.code = Some(existing.code);
            api::update_card(config, token, existing.code, &desired).await?;
            info!("Updated card {} for order {}", existing.code, order_code);
        } else {
            api::new_card(config, token, &desired).await?;
            info!("Created new card for order {} ({})", order_code, desired.functional_requirements);
        }
    }

    // Any remaining in existing_map should be deleted (they are no longer part of the order)
    for (req, obsolete) in existing_map {
        api::delete_card(config, token, obsolete.code).await?;
        info!("Deleted obsolete card {} for order {} ({})", obsolete.code, order_code, req);
    }

    Ok(())
}

async fn delete_cards(
    order_code: String,
    mysql_pool: &sqlx::MySqlPool,
    config: &Config,
    token: &str,
) -> Result<()> {
    let existing_activities = dealercrm::fetch_activities(mysql_pool, &order_code).await?;

    for act in existing_activities {
        api::delete_card(config, token, act.code).await?;
        info!("Deleted card {} for order {}", act.code, order_code);
    }

    Ok(())
}
