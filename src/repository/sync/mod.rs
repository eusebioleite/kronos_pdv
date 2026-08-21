use anyhow::{Context, Result};
use chrono::NaiveDate;
use sibyl::Row;
use std::collections::{BTreeMap, HashSet};
use tracing::info;

use crate::api::{self, Activity, ApiAttachment, ApiChat, WorkflowStage};
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
            problem: None,
            chats: None,
            attachments: None,
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

/// Merges user-generated fields from all CRM activities sharing the same planned_date.
///
/// - Problem: values joined with "\n---\n" (blank/None values are skipped).
/// - Chats: concatenated in per-activity order. Primary card's chats keep their GUID;
///   secondary card's chats have their GUID omitted (None) to avoid cross-parent update errors in CRM.
/// - Attachments: concatenated, deduped by GUID. Primary attachments keep their GUID;
///   secondary attachments have GUID omitted (None).
fn merge_user_data(
    acts: &[dealercrm::Activity],
    anchor_guid: &str,
) -> (Option<String>, Vec<ApiChat>, Vec<ApiAttachment>) {
    let mut problem_parts: Vec<&str> = Vec::new();
    let mut chats: Vec<ApiChat> = Vec::new();
    let mut attachments: Vec<ApiAttachment> = Vec::new();
    let mut seen_guids = std::collections::HashSet::<String>::new();

    for act in acts {
        let is_primary = act.activity_guid == anchor_guid;

        if let Some(p) = &act.activity_problem {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                problem_parts.push(trimmed);
            }
        }
        for c in &act.chats {
            chats.push(ApiChat {
                guid: if is_primary {
                    Some(c.activity_chat_guid.clone())
                } else {
                    None
                },
                person_code: c.activity_chat_person_code,
                text: c.activity_chat_text.clone(),
                comment_date: c
                    .activity_chat_comment_date
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
            });
        }
        for a in &act.attachments {
            if seen_guids.insert(a.activity_attachment_guid.clone()) {
                attachments.push(ApiAttachment {
                    guid: if is_primary {
                        Some(a.activity_attachment_guid.clone())
                    } else {
                        None
                    },
                    description: a.activity_attachment_description.clone(),
                });
            }
        }
    }

    let merged_problem = if problem_parts.is_empty() {
        None
    } else {
        Some(problem_parts.join("\n---\n"))
    };

    (merged_problem, chats, attachments)
}

/// Helper to extract schedule_codes from the JSON string stored in `business_rule`
fn parse_schedule_codes(raw: Option<&str>) -> HashSet<i64> {
    raw.and_then(|s| serde_json::from_str::<Vec<i64>>(s).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

/// Measures distance to arrival for a given date relative to `today`.
/// Returns (priority, diff):
/// - Upcoming / Today: (0, days_from_today_ascending) -> closest to arrive
/// - Past: (1, days_ago_ascending) -> closest to today among past dates
fn date_distance_to_arrival(date: NaiveDate, today: NaiveDate) -> (i64, i64) {
    let diff = (date - today).num_days();
    if diff >= 0 {
        (0, diff)
    } else {
        (1, -diff)
    }
}

/// Maps existing DealerCRM activities to new Oracle cards.
/// Returns a `Vec<Vec<dealercrm::Activity>>` of length `new_cards.len()`,
/// where index `i` contains all CRM activities that should be merged into `new_cards[i]`.
fn map_activities_to_new_cards(
    crm_acts: Vec<dealercrm::Activity>,
    new_cards: &[Activity],
    today: NaiveDate,
) -> Vec<Vec<dealercrm::Activity>> {
    if new_cards.is_empty() {
        return Vec::new();
    }

    let new_card_items: Vec<HashSet<i64>> = new_cards
        .iter()
        .map(|c| parse_schedule_codes(Some(&c.business_rule)))
        .collect();

    let mut buckets: Vec<Vec<dealercrm::Activity>> = vec![Vec::new(); new_cards.len()];

    for act in crm_acts {
        let act_items = parse_schedule_codes(act.activity_business_rule.as_deref());

        let mut best_intersection = 0;
        let mut candidate_indices = Vec::new();

        for (idx, items) in new_card_items.iter().enumerate() {
            let intersection = act_items.intersection(items).count();
            if intersection > best_intersection {
                best_intersection = intersection;
                candidate_indices.clear();
                candidate_indices.push(idx);
            } else if intersection == best_intersection && intersection > 0 {
                candidate_indices.push(idx);
            }
        }

        // If no schedule items intersected (e.g. items replaced/deleted, or empty business_rule),
        // all new cards are candidates, and the tie is broken by closest to arrive.
        if candidate_indices.is_empty() {
            candidate_indices = (0..new_cards.len()).collect();
        }

        // From candidate indices, pick the one with planned_date closest to arrive
        let chosen_idx = candidate_indices
            .into_iter()
            .min_by_key(|&idx| date_distance_to_arrival(new_cards[idx].planned_date, today))
            .unwrap_or(0);

        buckets[chosen_idx].push(act);
    }

    buckets
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

    // If order was cancelled / has no new cards in Oracle, delete all remaining CRM cards
    if new_cards.is_empty() {
        for obsolete in activities {
            info!(
                "DELETE obsolete card with guid {} for order '{order_code}'",
                obsolete.activity_guid
            );
            api::delete_card(&obsolete.activity_guid)
                .await
                .with_context(|| {
                    format!(
                        "Failed to DELETE card {} for order '{order_code}'",
                        obsolete.activity_guid
                    )
                })?;
        }
        crate::repository::queue::mark_sync(order_code)
            .await
            .with_context(|| format!("Failed to mark_sync for order '{order_code}'"))?;
        info!("Successfully synchronized order '{order_code}'.");
        return Ok(());
    }

    let today = chrono::Local::now().naive_local().date();
    let buckets = map_activities_to_new_cards(activities, &new_cards, today);

    for (new_card, acts) in new_cards.into_iter().zip(buckets) {
        if acts.is_empty() {
            // POST new card
            info!(
                "POST new card for order '{order_code}' on date {}",
                new_card.planned_date
            );
            api::new_card(&new_card).await.with_context(|| {
                format!(
                    "Failed to POST new card for order '{order_code}' on date {}",
                    new_card.planned_date
                )
            })?;
        } else {
            // Primary card is the first one in the bucket
            let primary = &acts[0];
            let (merged_problem, merged_chats, merged_attachments) =
                merge_user_data(&acts, &primary.activity_guid);

            let mut card_to_patch = new_card.clone();
            card_to_patch.problem = merged_problem;
            card_to_patch.chats = if merged_chats.is_empty() {
                None
            } else {
                Some(merged_chats)
            };
            card_to_patch.attachments = if merged_attachments.is_empty() {
                None
            } else {
                Some(merged_attachments)
            };

            info!(
                "PATCH card with guid {} for order '{order_code}' on date {}",
                primary.activity_guid, card_to_patch.planned_date
            );
            api::update_card(&primary.activity_guid, &card_to_patch)
                .await
                .with_context(|| {
                    format!(
                        "Failed to PATCH card {} for order '{order_code}' on date {}",
                        primary.activity_guid, card_to_patch.planned_date
                    )
                })?;

            // Clean up surplus/merged CRM cards
            for duplicate in acts.iter().skip(1) {
                info!(
                    "DELETE duplicate/merged card with guid {} for order '{order_code}'",
                    duplicate.activity_guid
                );
                api::delete_card(&duplicate.activity_guid)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to DELETE merged card {} for order '{order_code}'",
                            duplicate.activity_guid
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime};

    fn make_test_activity(
        code: i64,
        guid: &str,
        date: NaiveDate,
        items: Vec<i64>,
        problem: Option<&str>,
    ) -> dealercrm::Activity {
        dealercrm::Activity {
            activity_code: code,
            activity_guid: guid.to_string(),
            activity_title: Some(format!("Title {code}")),
            activity_detail: None,
            activity_functional_requirements: None,
            activity_business_rule: Some(serde_json::to_string(&items).unwrap()),
            activity_planned_date: Some(date),
            activity_problem: problem.map(|s| s.to_string()),
            chats: vec![],
            attachments: vec![],
        }
    }

    fn make_test_card(date: NaiveDate, items: Vec<i64>) -> Activity {
        Activity {
            guid: None,
            title: "Test Card".to_string(),
            detail: "Detail".to_string(),
            type_activity_code: 1,
            planned_date: date,
            replanned_date: date,
            objective: "Obj".to_string(),
            script: "Script".to_string(),
            functional_requirements: "0001".to_string(),
            business_rule: serde_json::to_string(&items).unwrap(),
            workflow_stages: None,
            problem: None,
            chats: None,
            attachments: None,
        }
    }

    #[test]
    fn test_merge_user_data() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 20).unwrap();
        let dt1 =
            NaiveDateTime::parse_from_str("2026-08-20 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let dt2 =
            NaiveDateTime::parse_from_str("2026-08-20 11:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

        let mut act1 = make_test_activity(101, "guid-1", date, vec![1], Some("Problem A"));
        act1.chats = vec![dealercrm::Chat {
            activity_chat_code: 1,
            activity_chat_guid: "chat-guid-1".to_string(),
            activity_code: 101,
            activity_chat_person_code: 42,
            activity_chat_text: "First message".to_string(),
            activity_chat_comment_date: dt1,
        }];
        act1.attachments = vec![dealercrm::Attachment {
            activity_attachment_code: 1,
            activity_attachment_guid: "att-guid-1".to_string(),
            activity_code: 101,
            activity_attachment_description: "Doc 1".to_string(),
        }];

        let mut act2 = make_test_activity(102, "guid-2", date, vec![2], Some("Problem B"));
        act2.chats = vec![dealercrm::Chat {
            activity_chat_code: 2,
            activity_chat_guid: "chat-guid-2".to_string(),
            activity_code: 102,
            activity_chat_person_code: 43,
            activity_chat_text: "Second message".to_string(),
            activity_chat_comment_date: dt2,
        }];
        act2.attachments = vec![
            dealercrm::Attachment {
                activity_attachment_code: 2,
                activity_attachment_guid: "att-guid-1".to_string(), // Duplicate GUID
                activity_code: 102,
                activity_attachment_description: "Doc 1 duplicate".to_string(),
            },
            dealercrm::Attachment {
                activity_attachment_code: 3,
                activity_attachment_guid: "att-guid-2".to_string(),
                activity_code: 102,
                activity_attachment_description: "Doc 2".to_string(),
            },
        ];

        let (problem, chats, attachments) = merge_user_data(&[act1, act2], "guid-1");

        assert_eq!(problem, Some("Problem A\n---\nProblem B".to_string()));
        assert_eq!(chats.len(), 2);
        assert_eq!(chats[0].guid, Some("chat-guid-1".to_string()));
        assert_eq!(chats[0].comment_date, "2026-08-20T10:00:00");
        // Secondary chat must have guid: None to prevent cross-parent update error in CRM
        assert_eq!(chats[1].guid, None);
        assert_eq!(chats[1].comment_date, "2026-08-20T11:00:00");

        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0].guid, Some("att-guid-1".to_string()));
        // Secondary attachment must have guid: None
        assert_eq!(attachments[1].guid, None);
    }

    #[test]
    fn test_merge_user_data_empty() {
        let (problem, chats, attachments) = merge_user_data(&[], "guid-1");
        assert_eq!(problem, None);
        assert!(chats.is_empty());
        assert!(attachments.is_empty());
    }

    #[test]
    fn test_map_activities_exact_and_merge() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
        let date1 = NaiveDate::from_ymd_opt(2026, 8, 25).unwrap();

        // 2 old CRM cards on different dates that merge into 1 new card in Oracle
        let act1 = make_test_activity(1, "guid-1", NaiveDate::from_ymd_opt(2026, 8, 20).unwrap(), vec![10], None);
        let act2 = make_test_activity(2, "guid-2", NaiveDate::from_ymd_opt(2026, 8, 22).unwrap(), vec![20], None);

        let new_card = make_test_card(date1, vec![10, 20]);

        let buckets = map_activities_to_new_cards(vec![act1, act2], &[new_card], today);

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].len(), 2);
        assert_eq!(buckets[0][0].activity_guid, "guid-1");
        assert_eq!(buckets[0][1].activity_guid, "guid-2");
    }

    #[test]
    fn test_map_activities_split_closest_to_arrive() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
        // New cards: Aug 25 (closest to arrive) vs Sep 10 (further)
        let new_card1 = make_test_card(NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(), vec![10]);
        let new_card2 = make_test_card(NaiveDate::from_ymd_opt(2026, 9, 10).unwrap(), vec![20]);

        // Old CRM card had both items [10, 20]
        let act = make_test_activity(1, "guid-split", NaiveDate::from_ymd_opt(2026, 8, 15).unwrap(), vec![10, 20], None);

        let buckets = map_activities_to_new_cards(vec![act], &[new_card1, new_card2], today);

        // Tied intersection (1 item in card 0, 1 item in card 1)
        // Should land in bucket 0 because Aug 25 is closest to arrive
        assert_eq!(buckets[0].len(), 1);
        assert_eq!(buckets[0][0].activity_guid, "guid-split");
        assert_eq!(buckets[1].len(), 0);
    }

    #[test]
    fn test_map_activities_deleted_item_fallback_closest_to_arrive() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
        let new_card1 = make_test_card(NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(), vec![100]);
        let new_card2 = make_test_card(NaiveDate::from_ymd_opt(2026, 8, 28).unwrap(), vec![200]);

        // Old CRM card had item [999] which was deleted from the order in Oracle
        let act = make_test_activity(1, "guid-deleted", NaiveDate::from_ymd_opt(2026, 8, 15).unwrap(), vec![999], None);

        let buckets = map_activities_to_new_cards(vec![act], &[new_card1, new_card2], today);

        // 0 intersection with both -> falls back to closest to arrive (Aug 25)
        assert_eq!(buckets[0].len(), 1);
        assert_eq!(buckets[0][0].activity_guid, "guid-deleted");
        assert_eq!(buckets[1].len(), 0);
    }
}

