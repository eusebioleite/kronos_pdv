use anyhow::{Context, Result};
use chrono::NaiveDate;
use sibyl::Row;
use std::collections::{BTreeMap, HashMap};
use tracing::info;

use crate::api::{self, Activity, WorkflowStage};
use crate::database;
use crate::dealercrm;
use crate::repository::queue::Queue;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Schedule {
    pub order_code: String,
    pub order_kind: String,
    pub schedule_code: i64,
    pub product_code: String,
    pub product_description: String,
    pub schedule_qtd: f64,
    pub product_bottle: f64,
    pub schedule_date: NaiveDate,
    pub company_code: i32,
    pub product_type: i32,
    pub order_type: String,
    pub delivery_type: String,
    pub order_nature: String,
    pub nature_description: String,
    pub order_seller: String,
    pub customer_code: String,
    pub customer_name: String,
    pub customer_fantasy: String,
}

impl Schedule {
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
                .get::<Option<String>, _>(3)
                .context("Failed to read column 3 (pro_codpro) from row")?
                .unwrap_or_default(),
            product_description: row
                .get::<Option<String>, _>(4)
                .context("Failed to read column 4 (pro_descri) from row")?
                .unwrap_or_default(),
            schedule_qtd: row
                .get(5)
                .context("Failed to read column 5 (prv_qtprog) from row")?,
            product_bottle: row
                .get(6)
                .context("Failed to read column 6 (pro_qtdemb) from row")?,
            schedule_date: {
                let date_str: String = row
                    .get::<Option<String>, _>(7)
                    .context("Failed to read column 7 (prv_dtprog) from row")?
                    .unwrap_or_default();
                NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                    .with_context(|| format!("Failed to parse schedule_date '{date_str}'"))?
            },
            company_code: row
                .get(8)
                .context("Failed to read column 8 (pdv_codseg) from row")?,
            product_type: row
                .get(9)
                .context("Failed to read column 9 (product_type) from row")?,
            order_type: row
                .get::<Option<String>, _>(10)
                .context("Failed to read column 10 (order_type) from row")?
                .unwrap_or_default(),
            delivery_type: row
                .get::<Option<String>, _>(11)
                .context("Failed to read column 11 (delivery_type) from row")?
                .unwrap_or_default(),
            order_nature: row
                .get::<Option<String>, _>(12)
                .context("Failed to read column 12 (order_nature) from row")?
                .unwrap_or_default(),
            nature_description: row
                .get::<Option<String>, _>(13)
                .context("Failed to read column 13 (nature_description) from row")?
                .unwrap_or_default(),
            order_seller: row
                .get::<Option<String>, _>(14)
                .context("Failed to read column 14 (order_seller) from row")?
                .unwrap_or_default(),
            customer_code: row
                .get(15)
                .context("Failed to read column 15 (customer_code) from row")?,
            customer_name: row
                .get::<Option<String>, _>(16)
                .context("Failed to read column 16 (customer_name) from row")?
                .unwrap_or_default(),
            customer_fantasy: row
                .get::<Option<String>, _>(17)
                .context("Failed to read column 17 (customer_fantasy) from row")?
                .unwrap_or_default(),
        })
    }
}

pub async fn get_new_cards(order_code: &str) -> Result<Vec<Activity>> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get Oracle session from pool")?;

    let sql = "
        SELECT 
            pdv_numped AS order_code,
            pdv_tipped AS order_kind,
            prv_indice AS schedule_code,
            pro_codpro AS product_code,
            pro_descri AS product_description,
            prv_qtprog AS schedule_qtd,
            pro_qtdemb AS product_bottle,
            TO_CHAR(prv_dtprog, 'YYYY-MM-DD') AS schedule_date,
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
        ORDER BY prv_dtprog, prv_indice
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare get_new_cards SQL statement")?;

    let rows = stmt
        .query(order_code)
        .await
        .with_context(|| format!("Failed to query schedules for order '{order_code}'"))?;

    let mut schedules: Vec<Schedule> = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .with_context(|| format!("Failed to fetch next schedule row for order '{order_code}'"))?
    {
        schedules.push(
            Schedule::from_row(&row).with_context(|| {
                format!("Failed to parse schedule row for order '{order_code}'")
            })?,
        );
    }

    if schedules.is_empty() {
        return Ok(Vec::new());
    }

    // Group by schedule_date (BTreeMap preserves sorted date order)
    let mut groups: BTreeMap<NaiveDate, Vec<Schedule>> = BTreeMap::new();
    for s in schedules {
        groups.entry(s.schedule_date).or_default().push(s);
    }

    let root_config = crate::config::get();
    let mut cards = Vec::new();

    for (date, group) in groups {
        let first = &group[0];
        let title = format!(
            "PDV {} | {}",
            first.order_code.trim_start_matches('0'),
            first.customer_name
        );
        let detail = build_detail(&group);
        let type_activity_code = first.product_type as i64;
        let schedule_codes: Vec<i64> = group.iter().map(|s| s.schedule_code).collect();
        let business_rule =
            serde_json::to_string(&schedule_codes).unwrap_or_else(|_| "[]".to_string());

        let default_col =
            root_config.get_column_by_company(first.company_code as u32, "PEDIDO EM CARTEIRA");
        let requester_code = root_config
            .get_requester_by_name(&first.order_seller)
            .map(|r| r.code)
            .unwrap_or_else(|| root_config.default_requester_code());

        let product_kind = match first.product_type {
            2 | 4 | 7 => "FRASCO",
            _ => "PREFORMA",
        };

        let responsible_code = if let (Some(col), Some(comp)) = (
            default_col,
            root_config
                .company
                .values()
                .find(|c| c.code == first.company_code as u32),
        ) {
            root_config.get_responsible(comp, col, requester_code, product_kind)
        } else {
            requester_code
        };

        let stage_code = default_col.map(|c| c.code as i64);

        let workflow_stages = Some(vec![WorkflowStage {
            workflow_stages_code: stage_code,
            code: stage_code,
            order: Some(0),
            order_to: Some(0),
            requester_person_code: Some(requester_code as i64),
            responsible_person_code: Some(responsible_code as i64),
        }]);

        cards.push(Activity {
            guid: None, // set by update_card when PATCHing an existing card
            title,
            detail,
            type_activity_code,
            planned_date: date,
            replanned_date: date,
            objective: first.delivery_type.clone(),
            script: first.customer_fantasy.clone(),
            functional_requirements: first.order_code.clone(),
            business_rule,
            workflow_stages,
        });
    }

    Ok(cards)
}

pub fn build_detail(schedules: &[Schedule]) -> String {
    let mut detail = String::new();

    for s in schedules {
        let schedule_code = s.schedule_code;
        let schedule_qtd = s.schedule_qtd;
        let product_bottle = s.product_bottle;
        let product_code = &s.product_code;
        let product_description = &s.product_description;

        detail.push_str(&format!(
            "• <strong>📋 Índice:</strong> <em>{schedule_code}</em><br>\n"
        ));
        detail.push_str(&format!(
            "• <strong>📦 Produto:</strong> <em>{product_code} | {product_description}</em><br>\n"
        ));
        detail.push_str("• <strong>🔢 Quantidade:</strong> <em>");

        if product_bottle == 0.0 {
            detail.push_str(&format!("{schedule_qtd} unidades no total."));
        } else {
            let boxes = (schedule_qtd / product_bottle).floor();
            let surplus = schedule_qtd % product_bottle;

            if boxes == 0.0 {
                detail.push_str(&format!("1 volume com {schedule_qtd} unidades no total."));
            } else if surplus > 0.0 {
                detail.push_str(&format!(
                    "{boxes} volumes de {product_bottle} unidades + 1 volume com {surplus} unidades ({schedule_qtd} unidades no total).\n"
                ));
            } else {
                detail.push_str(&format!(
                    "{boxes} volumes de {product_bottle} unidades ({schedule_qtd} unidades no total).\n"
                ));
            }
        }

        detail.push_str("</em><br><br>\n");
    }

    detail
}

pub async fn sync_queue(item: &Queue) -> Result<()> {
    let order_code = &item.order_code;

    // 1. Fetch ground truth from Oracle (grouped by schedule_date)
    let new_cards = get_new_cards(order_code)
        .await
        .with_context(|| format!("Failed to get new cards from Oracle for order '{order_code}'"))?;

    // 2. Fetch current activities from DealerCRM MySQL
    let activities = dealercrm::fetch_activities(order_code)
        .await
        .with_context(|| format!("Failed to fetch CRM activities for order '{order_code}'"))?;

    // 3. Build lookup maps keyed by planned_date
    let mut activities_map: HashMap<NaiveDate, Vec<dealercrm::Activity>> = HashMap::new();
    for act in activities {
        if let Some(date) = act.activity_planned_date {
            activities_map.entry(date).or_default().push(act);
        }
    }

    let new_cards_map: HashMap<NaiveDate, Activity> =
        new_cards.into_iter().map(|c| (c.planned_date, c)).collect();

    // 4a. POST: date exists in Oracle (new_cards) but not in DealerCRM (activities)
    for (date, new_card) in &new_cards_map {
        if !activities_map.contains_key(date) {
            info!("POST new card for order '{order_code}' on date {date}");
            api::new_card(new_card).await.with_context(|| {
                format!("Failed to POST new card for order '{order_code}' on date {date}")
            })?;
        }
    }

    // 4b. PATCH: date exists in both Oracle and DealerCRM (overwrite primary, cleanup surplus duplicates)
    for (date, new_card) in &new_cards_map {
        if let Some(acts) = activities_map.get(date) {
            if let Some(primary) = acts.first() {
                info!(
                    "PATCH card with guid {} for order '{order_code}' on date {date}",
                    primary.activity_guid
                );
                api::update_card(&primary.activity_guid, new_card)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to PATCH card {} for order '{order_code}' on date {date}",
                            primary.activity_guid
                        )
                    })?;
            }

            // If there are duplicate cards in DealerCRM on the same date, clean them up
            for duplicate in acts.iter().skip(1) {
                info!(
                    "DELETE duplicate card with guid {} for order '{order_code}' on date {date}",
                    duplicate.activity_guid
                );
                api::delete_card(&duplicate.activity_guid)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to DELETE duplicate card {} for order '{order_code}' on date {date}",
                            duplicate.activity_guid
                        )
                    })?;
            }
        }
    }

    // 4c. DELETE: date exists in DealerCRM but no longer in Oracle
    for (date, acts) in &activities_map {
        if !new_cards_map.contains_key(date) {
            for obsolete in acts {
                info!(
                    "DELETE obsolete card with guid {} for order '{order_code}' on date {date}",
                    obsolete.activity_guid
                );
                api::delete_card(&obsolete.activity_guid)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to DELETE card {} for order '{order_code}' on date {date}",
                            obsolete.activity_guid
                        )
                    })?;
            }
        }
    }

    // 5. Mark row as synced
    crate::repository::queue::mark_sync(order_code)
        .await
        .with_context(|| format!("Failed to mark_sync for order '{order_code}'"))?;

    info!("Successfully synchronized order '{order_code}'.");
    Ok(())
}
