use anyhow::{Context, Result};
use sibyl::{Row, Session};
use std::collections::HashMap;
use tracing::info;

use crate::api::{self, ActivityComplete};
use crate::config::Config;
use crate::dealercrm;
use crate::repository::queue::Queue;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Order {
    pub order_code: String,
    pub order_kind: String,
    pub schedule_code: String,
    pub product_code: String,
    pub product_description: String,
    pub schedule_qtd: f64,
    pub product_bottle: f64,
    pub schedule_date: chrono::NaiveDate,
    pub company_code: i32,
    pub product_type: i32,
    pub order_type: String,
    pub delivery_type: String,
    pub order_nature: String,
    pub nature_description: String,
    pub order_seller: String,
    pub customer_code: i32,
    pub customer_name: String,
    pub customer_fantasy: String,
}

impl Order {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            order_code: row
                .get::<Option<String>, _>(0)
                .context("Failed to read column 0 (pdv_numped) from row")?
                .unwrap_or_default(),
            order_kind: row
                .get::<Option<String>, _>(1)
                .context("Failed to read column 1 (pdv_tipped) from row")?
                .unwrap_or_default(),
            schedule_code: row
                .get(2)
                .context("Failed to read column 2 (prv_indice) from row")?,
            product_code: row
                .get(3)
                .context("Failed to read column 3 (prv_seqite) from row")?,
            product_description: row
                .get(4)
                .context("Failed to read column 4 (prv_codpro) from row")?,
            schedule_qtd: row
                .get(5)
                .context("Failed to read column 5 (prv_codnat) from row")?,
            product_bottle: row
                .get(6)
                .context("Failed to read column 6 (nat_descri) from row")?,
            schedule_date: row
                .get(7)
                .context("Failed to read column 7 (ped_codcli) from row")?,
            company_code: row
                .get(8)
                .context("Failed to read column 8 (cli_razao) from row")?,
            product_type: row
                .get(9)
                .context("Failed to read column 9 (prv_qtde) from row")?,
            order_type: row
                .get(10)
                .context("Failed to read column 10 (order_type) from row")?,
            delivery_type: row
                .get(11)
                .context("Failed to read column 11 (delivery_type) from row")?,
            order_nature: row
                .get(12)
                .context("Failed to read column 12 (order_nature) from row")?,
            nature_description: row
                .get(13)
                .context("Failed to read column 13 (nature_description) from row")?,
            order_seller: row
                .get(14)
                .context("Failed to read column 14 (order_seller) from row")?,
            customer_code: row
                .get(15)
                .context("Failed to read column 15 (customer_code) from row")?,
            customer_name: row
                .get(16)
                .context("Failed to read column 16 (customer_name) from row")?,
            customer_fantasy: row
                .get(17)
                .context("Failed to read column 17 (customer_fantasy) from row")?,
        })
    }
}

pub async fn build_cards(session: &Session<'_>, order_code: &str) -> Result<Vec<ActivityComplete>> {
    let sql = "
        SELECT 
            pdv_numped AS order_code,
            pdv_tipped AS order_kind,
            prv_indice AS schedule_code,
            pro_codpro AS product_code,
            pro_descri AS product_description,
            prv_qtprog AS schedule_qtd,
            pro_qtdemb AS product_bottle,
            prv_dtprog AS schedule_date,
            COALESCE(pdv_codseg, 100) AS company_code,
            CASE
                WHEN UPPER(pro_descri) LIKE '%FRASCO%' AND UPPER(nat_descri) LIKE '%AMOSTRA%' THEN 4
                WHEN UPPER(pro_descri) LIKE '%FRASCO%' AND UPPER(nat_descri) LIKE '%TRANSF%'  THEN 7
                WHEN UPPER(pro_descri) LIKE '%FRASCO%' THEN 2
                WHEN UPPER(nat_descri) LIKE '%AMOSTRA%' THEN 3
                WHEN UPPER(nat_descri) LIKE '%TRANSF%'  THEN 6
                ELSE 1
            END AS product_type,
            CASE 
                WHEN UPPER(nat_descri) LIKE '%AMOSTRA%'     THEN 'AMOSTRA'
                WHEN UPPER(nat_descri) LIKE '%TRANSF%'      THEN 'TRANSF'
                WHEN UPPER(nat_descri) LIKE '%VENDA%'       THEN 'VENDA'
                WHEN UPPER(nat_descri) LIKE '%APONTAMENTO%' THEN 'VENDA' 
                ELSE 'OUTROS' 
            END AS order_type,
            CASE pdv_tipent
                WHEN '2' THEN 'ENTREGA'
                ELSE 'COLETA'
            END AS delivery_type,
            pdv_indnat AS order_nature,
            nat_descri AS nature_description,
            pdv_vended AS order_seller,
            pdv_codemp AS customer_code,
            emp_erazao AS customer_name,
            emp_nfanta AS customer_fantasy
            FROM f_prgven
            JOIN f_pedvenda    ON prv_numped = pdv_numped
            LEFT JOIN f_cdemp  ON pdv_codemp = emp_codemp
            LEFT JOIN f_natope ON pdv_indnat = nat_indice
            LEFT JOIN f_prods  ON prv_codpro = pro_codpro
        WHERE pdv_numped = :1
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare build_cards SQL statement")?;
    let rows = stmt
        .query(order_code)
        .await
        .with_context(|| format!("Failed to query order details for order '{}'", order_code))?;

    let mut orders = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .with_context(|| format!("Failed to fetch next order row for order '{}'", order_code))?
    {
        orders.push(
            Order::from_row(&row)
                .with_context(|| format!("Failed to parse row for order '{}'", order_code))?,
        );
    }

    // Config defaults should ideally be passed, but we'll mock them for ActivityComplete
    let mut cards = Vec::new();

    // Grouping logic (PDV-A grouped by date, PDV-F grouped into one)
    let mut grouped: HashMap<String, Vec<Order>> = HashMap::new();

    for order in orders {
        let key = if order.order_kind == "PDV-A" {
            order
                .delivery_date
                .clone()
                .unwrap_or_else(|| "1970-01-01".to_string())
        } else {
            "ALL".to_string()
        };
        grouped.entry(key).or_default().push(order);
    }

    for (date_key, group) in grouped {
        let first = &group[0];
        let title = format!(
            "Pedido {} - Cliente {}",
            first.order_code,
            first.customer_name.as_deref().unwrap_or("")
        );
        let func_req = if first.order_kind == "PDV-A" {
            format!("ERP-{}-{}", first.order_code, date_key)
        } else {
            format!("ERP-{}-FECHADO", first.order_code)
        };

        let mut detail = String::new();
        for item in &group {
            detail.push_str(&format!(
                "Item: {} Qtd: {}\n",
                item.item_code.as_deref().unwrap_or(""),
                item.quantity.unwrap_or(0.0)
            ));
        }

        cards.push(ActivityComplete {
            code: None,
            process_id: 1, // These should come from config
            title,
            detail,
            planned_date: if date_key == "ALL" {
                "1970-01-01".to_string()
            } else {
                date_key
            },
            functional_requirements: func_req,
            requester_id: 1, // From config
            seller_id: 1,    // From config
            company_id: 1,   // From config
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
    api_client: &reqwest_middleware::ClientWithMiddleware,
) -> Result<()> {
    if card.status == "EXCLUIR" {
        delete_cards(card.pedido.clone(), mysql_pool, config, api_client)
            .await
            .with_context(|| format!("Failed to delete CRM cards for pedido '{}'", card.pedido))?;
        crate::repository::queue::update_status(session, "EXCLUIDO", card)
            .await
            .with_context(|| {
                format!(
                    "Failed to update queue status to EXCLUIDO for pedido '{}'",
                    card.pedido
                )
            })?;
        return Ok(());
    }

    let new_cards = build_cards(session, &card.pedido).await.with_context(|| {
        format!(
            "Failed to build new cards from Oracle for pedido '{}'",
            card.pedido
        )
    })?;

    sync_cards(&card.pedido, desired_cards, mysql_pool, config, api_client)
        .await
        .with_context(|| {
            format!(
                "Failed to sync cards to DealerCRM/API for pedido '{}'",
                card.pedido
            )
        })?;

    crate::repository::queue::update_status(session, "SUCESSO", card)
        .await
        .with_context(|| {
            format!(
                "Failed to update queue status to SUCESSO for pedido '{}'",
                card.pedido
            )
        })?;

    Ok(())
}

async fn sync_cards(
    order_code: &str,
    desired_cards: Vec<ActivityComplete>,
    mysql_pool: &sqlx::MySqlPool,
    config: &Config,
    api_client: &reqwest_middleware::ClientWithMiddleware,
) -> Result<()> {
    let existing_activities = dealercrm::fetch_activities(mysql_pool, order_code)
        .await
        .with_context(|| {
            format!(
                "Failed to fetch existing CRM activities for order '{}'",
                order_code
            )
        })?;

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
            api::update_card(api_client, config, existing.code, &desired)
                .await
                .with_context(|| {
                    format!(
                        "Failed to update API card {} for order '{}'",
                        existing.code, order_code
                    )
                })?;
            info!("Updated card {} for order {}", existing.code, order_code);
        } else {
            api::new_card(api_client, config, &desired)
                .await
                .with_context(|| {
                    format!(
                        "Failed to create API card for order '{}' ({})",
                        order_code, desired.functional_requirements
                    )
                })?;
            info!(
                "Created new card for order {} ({})",
                order_code, desired.functional_requirements
            );
        }
    }

    // Any remaining in existing_map should be deleted (they are no longer part of the order)
    for (req, obsolete) in existing_map {
        api::delete_card(api_client, config, obsolete.code)
            .await
            .with_context(|| {
                format!(
                    "Failed to delete obsolete API card {} for order '{}' ({})",
                    obsolete.code, order_code, req
                )
            })?;
        info!(
            "Deleted obsolete card {} for order {} ({})",
            obsolete.code, order_code, req
        );
    }

    Ok(())
}

async fn delete_cards(
    order_code: String,
    mysql_pool: &sqlx::MySqlPool,
    config: &Config,
    api_client: &reqwest_middleware::ClientWithMiddleware,
) -> Result<()> {
    let existing_activities = dealercrm::fetch_activities(mysql_pool, &order_code)
        .await
        .with_context(|| {
            format!(
                "Failed to fetch existing CRM activities for order '{}' to delete",
                order_code
            )
        })?;

    for act in existing_activities {
        api::delete_card(api_client, config, act.code)
            .await
            .with_context(|| {
                format!(
                    "Failed to delete card {} for order '{}'",
                    act.code, order_code
                )
            })?;
        info!("Deleted card {} for order {}", act.code, order_code);
    }

    Ok(())
}
