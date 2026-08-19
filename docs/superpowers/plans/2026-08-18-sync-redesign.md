# Kronos PDV Sync Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the status-based sync with a pure set-reconciliation model — diff Oracle vs DealerCRM by `planned_date`, then POST/PATCH/DELETE.

**Architecture:** One queue row per `order_code` with a `schedules JSON` array maintained by Oracle triggers. Rust reads all current Oracle schedules fresh on each sync, groups by date, compares against DealerCRM activities, and applies the minimal diff.

**Tech Stack:** Rust, sibyl (Oracle), sqlx (MySQL), reqwest-middleware, serde_json, chrono, anyhow

**Spec:** `docs/superpowers/specs/2026-08-18-sync-redesign-design.md`

## Global Constraints

- Oracle binding uses positional `:1`, `:2` (sibyl). MySQL uses `?` (sqlx).
- PATCH never sends `workflow_stages` — preserves CRM column.
- DELETE uses `activity_guid` (String), not `activity_code`.
- `Activity_FunctionalRequirements` = `order_code` (string, e.g. `"0002031"`).
- `Activity_BusinessRule` = JSON array of i64s, e.g. `[1, 2, 3]`, serialized by serde.
- `business_rules: Vec<i64>` serializes to `[1,2,3]` via `#[serde(rename_all = "camelCase")]`.
- All functions return `anyhow::Result<T>` with `.context()` / `.with_context()`.
- No `todo!()` or `unreachable!()` left in final code.

---

### Task 1: Rewrite `repository/queue/mod.rs`

**Files:**
- Modify: `src/repository/queue/mod.rs`

**Interfaces:**
- Produces:
  - `pub struct Queue { pub order_code: String, pub retries: i32, pub error: Option<String> }`
  - `pub async fn get_queue() -> Result<Vec<Queue>>`
  - `pub async fn mark_synced(order_code: &str) -> Result<()>`
  - `pub async fn update_error(order_code: &str, error: &str) -> Result<()>`

- [ ] **Step 1: Replace the `Queue` struct**

```rust
// src/repository/queue/mod.rs

use anyhow::{Context, Result};
use sibyl::Row;
use crate::database;

#[derive(Debug, Clone)]
pub struct Queue {
    pub order_code: String,
    pub retries: i32,
    pub error: Option<String>,
}

impl Queue {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        let order_code: String = row
            .get::<Option<String>, _>(0usize)
            .context("Failed to read 'order_code' (column 0) from row")?
            .unwrap_or_default();
        let retries: i32 = row
            .get::<Option<i32>, _>(1usize)
            .context("Failed to read 'retries' (column 1) from row")?
            .unwrap_or_default();
        let error: Option<String> = row
            .get(2usize)
            .context("Failed to read 'error' (column 2) from row")?;

        Ok(Self { order_code, retries, error })
    }
}
```

- [ ] **Step 2: Replace `get_queue()`**

```rust
pub async fn get_queue() -> Result<Vec<Queue>> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get session from pool.")?;

    let sql = "
        SELECT
            order_code,
            retries,
            TO_CHAR(error) AS error
        FROM kronos_pdv_queue
        WHERE sync = 1
          AND retries < 5
        ORDER BY updated_at
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare get_queue statement")?;

    let rows = stmt
        .query(())
        .await
        .context("Failed to query kronos_pdv_queue")?;

    let mut queue = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .context("Failed to fetch next row from queue query")?
    {
        queue.push(Queue::from_row(&row).context("Failed to parse Queue row")?);
    }

    Ok(queue)
}
```

- [ ] **Step 3: Add `mark_synced()`**

```rust
pub async fn mark_synced(order_code: &str) -> Result<()> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get session from pool.")?;

    let sql = "
        UPDATE kronos_pdv_queue
        SET sync = 0, retries = 0, updated_at = SYSTIMESTAMP
        WHERE order_code = :1
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare mark_synced statement")?;

    stmt.execute(order_code)
        .await
        .with_context(|| format!("Failed to mark_synced for order '{order_code}'"))?;

    session
        .commit()
        .await
        .with_context(|| format!("Failed to commit mark_synced for order '{order_code}'"))?;

    Ok(())
}
```

- [ ] **Step 4: Add `update_error()`**

```rust
pub async fn update_error(order_code: &str, error: &str) -> Result<()> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get session from pool.")?;

    let mut error_msg = error.to_string();
    if error_msg.len() > 4000 {
        error_msg.truncate(4000);
    }

    let sql = "
        UPDATE inventario.kronos_pdv_queue
        SET error = :1,
            retries    = retries + 1,
            updated_at = SYSTIMESTAMP
        WHERE order_code = :2
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare update_error statement")?;

    stmt.execute((error_msg.as_str(), order_code))
        .await
        .with_context(|| format!("Failed to update_error for order '{order_code}'"))?;

    session
        .commit()
        .await
        .with_context(|| format!("Failed to commit update_error for order '{order_code}'"))?;

    Ok(())
}
```

- [ ] **Step 5: Run `cargo check` — fix any compilation errors before continuing**

```
cargo check
```

Expected: no errors in `repository/queue/mod.rs`.

- [ ] **Step 6: Commit**

```
git add src/repository/queue/mod.rs
git commit -m "refactor(queue): simplify Queue struct and align SQL to new schema"
```

---

### Task 2: Rewrite `dealercrm/mod.rs`

**Files:**
- Modify: `src/dealercrm/mod.rs`

**Interfaces:**
- Produces:
  - `pub struct Activity { activity_code, activity_guid, activity_planned_date, ... }`
  - `pub async fn fetch_activities(order_code: &str) -> Result<Vec<Activity>>`
  - `pub fn get_pool() -> &'static MySqlPool` (unchanged)
  - `pub async fn init_pool() -> Result<()>` (unchanged)

- [ ] **Step 1: Replace the `Activity` struct and remove duplicate functions**

```rust
// src/dealercrm/mod.rs

use anyhow::{Context, Result};
use sqlx::{FromRow, MySqlPool};
use sqlx::mysql::MySqlPoolOptions;
use std::sync::OnceLock;

#[derive(Debug, Clone, FromRow)]
pub struct Activity {
    #[sqlx(rename = "Activity_Code")]
    pub activity_code: i64,
    #[sqlx(rename = "Activity_Guid")]
    pub activity_guid: String,
    #[sqlx(rename = "Activity_Title")]
    pub activity_title: Option<String>,
    #[sqlx(rename = "Activity_Detail")]
    pub activity_detail: Option<String>,
    #[sqlx(rename = "Activity_FunctionalRequirements")]
    pub activity_functional_requirements: Option<String>,
    #[sqlx(rename = "Activity_BusinessRule")]
    pub activity_business_rule: Option<String>,
    #[sqlx(rename = "Activity_PlannedDate")]
    pub activity_planned_date: Option<chrono::NaiveDate>,
}

static MYSQL_POOL: OnceLock<MySqlPool> = OnceLock::new();

pub fn get_pool() -> &'static MySqlPool {
    MYSQL_POOL.get().expect("dealercrm::init_pool() must be called before get_pool()")
}

pub async fn init_pool() -> Result<()> {
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect("mysql://dealercrm:123456@localhost:3306/dealercrm")
        .await
        .context("Failed to connect to MySQL DealerCRM")?;
    MYSQL_POOL
        .set(pool)
        .map_err(|_| anyhow::anyhow!("dealercrm::init_pool() was called more than once"))?;
    Ok(())
}
```

- [ ] **Step 2: Add `fetch_activities(order_code)`**

```rust
pub async fn fetch_activities(order_code: &str) -> Result<Vec<Activity>> {
    let pool = get_pool();

    // NOTE: Activity_FunctionalRequirements stores the order_code string.
    // The JOIN with ActivityWorkflowStages / WorkflowStages is kept only to
    // satisfy the relational requirement; we don't need those columns here.
    let sql = "
        SELECT DISTINCT
            a.Activity_Code,
            a.Activity_Guid,
            a.Activity_Title,
            a.Activity_Detail,
            a.Activity_FunctionalRequirements,
            a.Activity_BusinessRule,
            a.Activity_PlannedDate
        FROM Activity a
        JOIN ActivityWorkflowStages aws ON aws.ActivityWorkflowStages_ActivityCode = a.Activity_Code
        JOIN WorkflowStages ws ON aws.ActivityWorkflowStages_WorkflowStagesCode = ws.WorkflowStages_Code
        WHERE a.Activity_FunctionalRequirements = ?
    ";

    let activities = sqlx::query_as::<_, Activity>(sql)
        .bind(order_code)
        .fetch_all(pool)
        .await
        .with_context(|| format!("Failed to fetch activities for order '{order_code}'"))?;

    Ok(activities)
}
```

- [ ] **Step 3: Run `cargo check`**

```
cargo check
```

Expected: no errors in `dealercrm/mod.rs`.

- [ ] **Step 4: Commit**

```
git add src/dealercrm/mod.rs
git commit -m "refactor(dealercrm): simplify Activity struct, single fetch_activities fn"
```

---

### Task 3: Rewrite `api/mod.rs`

**Files:**
- Modify: `src/api/mod.rs`

**Interfaces:**
- Produces:
  - `pub struct WorkflowStage { workflow_stages_code: i64, code: i64, order: i64, order_to: i64 }`
  - `pub struct ActivityCreate { ..., business_rules: Vec<i64>, workflow_stages: Vec<WorkflowStage> }`
  - `pub struct ActivityUpdate { ..., business_rules: Vec<i64> }` (no workflow_stages)
  - `pub async fn new_card(client, config, card: &ActivityCreate) -> Result<()>`
  - `pub async fn update_card(client, config, code: i64, card: &ActivityUpdate) -> Result<()>`
  - `pub async fn delete_card(client, config, guid: &str) -> Result<()>`

- [ ] **Step 1: Replace all structs**

```rust
// src/api/mod.rs

use anyhow::{anyhow, Context, Result};
use reqwest_middleware::ClientWithMiddleware;
use serde::Serialize;
use crate::config::Config;

pub mod auth;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStage {
    pub workflow_stages_code: i64,
    pub code: i64,
    pub order: i64,
    pub order_to: i64,
}

/// POST payload — includes workflow_stages to initialize the card's workflow.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCreate {
    pub title: String,
    pub detail: String,
    pub type_activity_code: i64,
    pub planned_date: chrono::NaiveDate,
    pub replanned_date: chrono::NaiveDate,
    pub objective: String,
    pub script: String,
    pub functional_requirements: String,
    pub business_rules: Vec<i64>,
    pub workflow_stages: Vec<WorkflowStage>,
}

/// PATCH payload — NO workflow_stages (preserves CRM's workflow column).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityUpdate {
    pub title: String,
    pub detail: String,
    pub type_activity_code: i64,
    pub planned_date: chrono::NaiveDate,
    pub replanned_date: chrono::NaiveDate,
    pub objective: String,
    pub script: String,
    pub functional_requirements: String,
    pub business_rules: Vec<i64>,
}
```

- [ ] **Step 2: Replace the three API functions**

```rust
pub async fn new_card(
    client: &ClientWithMiddleware,
    config: &Config,
    card: &ActivityCreate,
) -> Result<()> {
    let url = format!("{}/v3/works/core/activities", config.api_url);
    let body = serde_json::to_vec(card)
        .context("Failed to serialize ActivityCreate to JSON")?;

    let res = client
        .post(&url)
        .header("ContextGuid", &config.context_guid)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .with_context(|| format!("Failed to POST new_card to '{url}'"))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!("new_card failed at '{url}': HTTP {status} — {body}"));
    }

    Ok(())
}

pub async fn update_card(
    client: &ClientWithMiddleware,
    config: &Config,
    code: i64,
    card: &ActivityUpdate,
) -> Result<()> {
    let url = format!("{}/v3/works/core/activities/{code}", config.api_url);
    let body = serde_json::to_vec(card)
        .with_context(|| format!("Failed to serialize ActivityUpdate for code {code}"))?;

    let res = client
        .patch(&url)
        .header("ContextGuid", &config.context_guid)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .with_context(|| format!("Failed to PATCH update_card to '{url}'"))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!("update_card {code} failed at '{url}': HTTP {status} — {body}"));
    }

    Ok(())
}

pub async fn delete_card(
    client: &ClientWithMiddleware,
    config: &Config,
    guid: &str,
) -> Result<()> {
    let url = format!("{}/v3/works/core/activities/{guid}", config.api_url);

    let res = client
        .delete(&url)
        .header("ContextGuid", &config.context_guid)
        .send()
        .await
        .with_context(|| format!("Failed to DELETE card '{guid}' at '{url}'"))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!("delete_card '{guid}' failed at '{url}': HTTP {status} — {body}"));
    }

    Ok(())
}
```

- [ ] **Step 3: Run `cargo check`**

```
cargo check
```

Expected: no errors in `api/mod.rs`. Note: `sync/mod.rs` will still have errors from old references — that's fine until Task 4.

- [ ] **Step 4: Commit**

```
git add src/api/mod.rs
git commit -m "refactor(api): replace ActivityComplete with ActivityCreate/ActivityUpdate; fix delete to use GUID"
```

---

### Task 4: Rewrite `repository/sync/mod.rs`

This is the core task. Complete rewrite of the file.

**Files:**
- Modify: `src/repository/sync/mod.rs`

**Interfaces:**
- Consumes:
  - `crate::dealercrm::fetch_activities(order_code: &str) -> Result<Vec<Activity>>`
  - `crate::api::{new_card, update_card, delete_card, ActivityCreate, ActivityUpdate, WorkflowStage}`
  - `crate::repository::queue::mark_synced(order_code: &str) -> Result<()>`
  - `crate::config::Config` with `config.default_stage_code: i64`
  - `crate::database::get_pool()`
- Produces:
  - `pub async fn sync_queue(item: &Queue, config: &Config, client: &ClientWithMiddleware) -> Result<()>`

- [ ] **Step 1: Write the new `Schedule` struct and `from_row`**

```rust
// src/repository/sync/mod.rs

use anyhow::{Context, Result};
use chrono::NaiveDate;
use reqwest_middleware::ClientWithMiddleware;
use sibyl::Row;
use std::collections::HashMap;
use tracing::info;

use crate::api::{self, ActivityCreate, ActivityUpdate, WorkflowStage};
use crate::config::Config;
use crate::{database, dealercrm};
use crate::repository::queue::Queue;

#[derive(Debug, Clone)]
pub struct Schedule {
    pub order_code: String,
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
    pub customer_code: i32,
    pub customer_name: String,
    pub customer_fantasy: String,
}

impl Schedule {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            order_code: row
                .get::<Option<String>, _>(0)
                .context("col 0 order_code")?
                .unwrap_or_default(),
            schedule_code: row.get(1).context("col 1 schedule_code")?,
            product_code: row.get(2).context("col 2 product_code")?,
            product_description: row.get(3).context("col 3 product_description")?,
            schedule_qtd: row.get(4).context("col 4 schedule_qtd")?,
            product_bottle: row.get(5).context("col 5 product_bottle")?,
            schedule_date: row.get(6).context("col 6 schedule_date")?,
            company_code: row.get(7).context("col 7 company_code")?,
            product_type: row.get(8).context("col 8 product_type")?,
            order_type: row.get(9).context("col 9 order_type")?,
            delivery_type: row.get(10).context("col 10 delivery_type")?,
            order_nature: row
                .get::<Option<String>, _>(11)
                .context("col 11 order_nature")?
                .unwrap_or_default(),
            nature_description: row
                .get::<Option<String>, _>(12)
                .context("col 12 nature_description")?
                .unwrap_or_default(),
            order_seller: row
                .get::<Option<String>, _>(13)
                .context("col 13 order_seller")?
                .unwrap_or_default(),
            customer_code: row.get(14).context("col 14 customer_code")?,
            customer_name: row
                .get::<Option<String>, _>(15)
                .context("col 15 customer_name")?
                .unwrap_or_default(),
            customer_fantasy: row
                .get::<Option<String>, _>(16)
                .context("col 16 customer_fantasy")?
                .unwrap_or_default(),
        })
    }
}
```

- [ ] **Step 2: Write `get_new_cards(order_code)`**

```rust
async fn get_new_cards(order_code: &str, config: &Config) -> Result<Vec<ActivityCreate>> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get Oracle session for get_new_cards")?;

    let sql = "
        SELECT
            pdv_numped                          AS order_code,
            prv_indice                          AS schedule_code,
            pro_codpro                          AS product_code,
            pro_descri                          AS product_description,
            prv_qtprog                          AS schedule_qtd,
            pro_qtdemb                          AS product_bottle,
            prv_dtprog                          AS schedule_date,
            COALESCE(pdv_codseg, 100)           AS company_code,
            CASE
                WHEN UPPER(pro_descri) LIKE '%FRASCO%' AND UPPER(nat_descri) LIKE '%AMOSTRA%' THEN 4
                WHEN UPPER(pro_descri) LIKE '%FRASCO%' AND UPPER(nat_descri) LIKE '%TRANSF%'  THEN 7
                WHEN UPPER(pro_descri) LIKE '%FRASCO%'                                        THEN 2
                WHEN UPPER(nat_descri) LIKE '%AMOSTRA%'                                       THEN 3
                WHEN UPPER(nat_descri) LIKE '%TRANSF%'                                        THEN 6
                ELSE 1
            END                                 AS product_type,
            CASE
                WHEN UPPER(nat_descri) LIKE '%AMOSTRA%'     THEN 'AMOSTRA'
                WHEN UPPER(nat_descri) LIKE '%TRANSF%'      THEN 'TRANSF'
                WHEN UPPER(nat_descri) LIKE '%VENDA%'       THEN 'VENDA'
                WHEN UPPER(nat_descri) LIKE '%APONTAMENTO%' THEN 'VENDA'
                ELSE 'OUTROS'
            END                                 AS order_type,
            CASE pdv_tipent WHEN '2' THEN 'ENTREGA' ELSE 'COLETA' END AS delivery_type,
            pdv_indnat                          AS order_nature,
            nat_descri                          AS nature_description,
            pdv_vended                          AS order_seller,
            pdv_codemp                          AS customer_code,
            emp_erazao                          AS customer_name,
            emp_nfanta                          AS customer_fantasy
        FROM f_prgven
        JOIN  f_pedvenda ON prv_numped  = pdv_numped
        LEFT JOIN f_cdemp   ON pdv_codemp  = emp_codemp
        LEFT JOIN f_natope  ON pdv_indnat  = nat_indice
        LEFT JOIN f_prods   ON prv_codpro  = pro_codpro
        WHERE pdv_numped = :1
        ORDER BY prv_dtprog, prv_indice
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare get_new_cards SQL")?;

    let rows = stmt
        .query(order_code)
        .await
        .with_context(|| format!("Failed to query schedules for order '{order_code}'"))?;

    let mut schedules: Vec<Schedule> = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .with_context(|| format!("Failed to fetch schedule row for order '{order_code}'"))?
    {
        schedules.push(
            Schedule::from_row(&row)
                .with_context(|| format!("Failed to parse schedule row for order '{order_code}'"))?,
        );
    }

    // Group by schedule_date (BTreeMap preserves date order)
    let mut groups: std::collections::BTreeMap<NaiveDate, Vec<Schedule>> =
        std::collections::BTreeMap::new();
    for s in schedules {
        groups.entry(s.schedule_date).or_default().push(s);
    }

    // Build one ActivityCreate per date group
    let mut cards: Vec<ActivityCreate> = Vec::new();
    for (date, group) in groups {
        let first = &group[0];
        let title = format!(
            "PDV {} | {}",
            first.order_code.trim_start_matches('0'),
            first.customer_name
        );
        let detail = build_detail(&group);
        let type_activity_code = get_activity_code(first);
        let business_rules: Vec<i64> = group.iter().map(|s| s.schedule_code).collect();

        cards.push(ActivityCreate {
            title,
            detail,
            type_activity_code,
            planned_date: date,
            replanned_date: date,
            objective: first.delivery_type.clone(),
            script: first.customer_fantasy.clone(),
            functional_requirements: first.order_code.clone(),
            business_rules,
            workflow_stages: vec![WorkflowStage {
                workflow_stages_code: config.default_stage_code,
                code: config.default_stage_code,
                order: 1,
                order_to: 0,
            }],
        });
    }

    Ok(cards)
}
```

- [ ] **Step 3: Write helper functions `build_detail` and `get_activity_code`**

```rust
fn build_detail(schedules: &[Schedule]) -> String {
    let mut detail = String::new();
    for s in schedules {
        detail.push_str(&format!(
            "• <strong>📋 Índice:</strong> <em>{}</em><br>\n",
            s.schedule_code
        ));
        detail.push_str(&format!(
            "• <strong>📦 Produto:</strong> <em>{} | {}</em><br>\n",
            s.product_code, s.product_description
        ));
        detail.push_str("• <strong>🔢 Quantidade:</strong> <em>");

        let qty_text = if s.product_bottle == 0.0 {
            format!("{} unidades no total", s.schedule_qtd)
        } else {
            let boxes = (s.schedule_qtd / s.product_bottle).floor();
            let surplus = s.schedule_qtd % s.product_bottle;
            if boxes == 0.0 {
                format!("1 volume com {} unidades no total", s.schedule_qtd)
            } else if surplus > 0.0 {
                format!(
                    "{boxes} volumes de {} unidades + 1 volume com {surplus} unidades ({} unidades no total)",
                    s.product_bottle, s.schedule_qtd
                )
            } else {
                format!(
                    "{boxes} volumes de {} unidades ({} unidades no total)",
                    s.product_bottle, s.schedule_qtd
                )
            }
        };

        detail.push_str(&qty_text);
        detail.push_str("</em><br><br>\n");
    }
    detail
}

fn get_activity_code(s: &Schedule) -> i64 {
    match s.product_type {
        4 => 4, // FRASCO + AMOSTRA
        7 => 7, // FRASCO + TRANSF
        2 => 2, // FRASCO + VENDA
        3 => 3, // PREFORMA + AMOSTRA
        6 => 6, // PREFORMA + TRANSF
        1 => 1, // PREFORMA + VENDA
        _ => 1,
    }
}
```

- [ ] **Step 4: Write `sync_queue()` — the diff + dispatch**

```rust
pub async fn sync_queue(
    item: &Queue,
    config: &Config,
    client: &ClientWithMiddleware,
) -> Result<()> {
    let order_code = &item.order_code;

    // 1. Fetch ground truth from Oracle (grouped by date)
    let new_cards = get_new_cards(order_code, config)
        .await
        .with_context(|| format!("Failed to get new cards from Oracle for order '{order_code}'"))?;

    // 2. Fetch current state from DealerCRM
    let activities = dealercrm::fetch_activities(order_code)
        .await
        .with_context(|| format!("Failed to fetch CRM activities for order '{order_code}'"))?;

    // 3. Build lookup maps keyed by planned_date
    let activities_map: HashMap<NaiveDate, dealercrm::Activity> = activities
        .into_iter()
        .filter_map(|a| a.activity_planned_date.map(|d| (d, a)))
        .collect();

    let new_cards_map: HashMap<NaiveDate, ActivityCreate> = new_cards
        .into_iter()
        .map(|c| (c.planned_date, c))
        .collect();

    // 4a. POST — dates in Oracle but missing from CRM
    for (date, new_card) in &new_cards_map {
        if !activities_map.contains_key(date) {
            info!("POST new card for order '{order_code}' on date {date}");
            api::new_card(client, config, new_card)
                .await
                .with_context(|| {
                    format!("Failed to POST card for order '{order_code}' on date {date}")
                })?;
        }
    }

    // 4b. PATCH — dates in both (Oracle overwrites CRM)
    for (date, new_card) in &new_cards_map {
        if let Some(existing) = activities_map.get(date) {
            let update = ActivityUpdate {
                title: new_card.title.clone(),
                detail: new_card.detail.clone(),
                type_activity_code: new_card.type_activity_code,
                planned_date: new_card.planned_date,
                replanned_date: new_card.replanned_date,
                objective: new_card.objective.clone(),
                script: new_card.script.clone(),
                functional_requirements: new_card.functional_requirements.clone(),
                business_rules: new_card.business_rules.clone(),
            };
            info!(
                "PATCH card {} for order '{order_code}' on date {date}",
                existing.activity_code
            );
            api::update_card(client, config, existing.activity_code, &update)
                .await
                .with_context(|| {
                    format!(
                        "Failed to PATCH card {} for order '{order_code}' on date {date}",
                        existing.activity_code
                    )
                })?;
        }
    }

    // 4c. DELETE — dates in CRM but no longer in Oracle
    for (date, obsolete) in &activities_map {
        if !new_cards_map.contains_key(date) {
            info!(
                "DELETE card {} ({}) for order '{order_code}' on date {date}",
                obsolete.activity_code, obsolete.activity_guid
            );
            api::delete_card(client, config, &obsolete.activity_guid)
                .await
                .with_context(|| {
                    format!(
                        "Failed to DELETE card {} for order '{order_code}' on date {date}",
                        obsolete.activity_code
                    )
                })?;
        }
    }

    // 5. Mark the queue row as synced
    crate::repository::queue::mark_synced(order_code)
        .await
        .with_context(|| format!("Failed to mark_synced for order '{order_code}'"))?;

    info!("Sync complete for order '{order_code}'");
    Ok(())
}
```

- [ ] **Step 5: Run `cargo check`**

```
cargo check
```

Expected: no errors. If `config.default_stage_code` does not exist yet on the Config struct, add `pub default_stage_code: i64` to `src/config.rs` and any config file/struct. Fix any type mismatches before continuing.

- [ ] **Step 6: Commit**

```
git add src/repository/sync/mod.rs
git commit -m "feat(sync): complete rewrite — set-reconciliation diff with POST/PATCH/DELETE"
```

---

### Task 5: Align `main.rs` call sites

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes:
  - `repository::queue::get_queue() -> Result<Vec<Queue>>`
  - `repository::sync::sync_queue(item: &Queue, config: &Config, client: &ClientWithMiddleware) -> Result<()>`
  - `repository::queue::update_error(order_code: &str, error: &str) -> Result<()>`

- [ ] **Step 1: Fix `main.rs` to match new signatures**

The existing `main.rs` calls `sync_queue(&item)` with one argument and references `card.pedido` (old field name). Update to:

```rust
// In the processing loop:
for item in queue {
    info!("Processing order: {}", item.order_code);

    match repository::sync::sync_queue(&item, config::get(), api_client).await {
        Ok(_) => {
            info!("Order '{}' synced successfully.", item.order_code);
        }
        Err(e) => {
            error!("Error syncing order '{}': {e:#}", item.order_code);
            match repository::queue::update_error(&item.order_code, &e.to_string()).await {
                Ok(_) => info!("Last error recorded for order '{}'.", item.order_code),
                Err(err) => error!(
                    "Failed to record last error for order '{}': {err}",
                    item.order_code
                ),
            }
        }
    }

    if config::get().config.throttle > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(
            config::get().config.throttle as u64,
        ))
        .await;
    }
}
```

Note: `api_client` must be built somewhere in `main()`. If it isn't yet, add:

```rust
let api_client = reqwest_middleware::ClientBuilder::new(reqwest::Client::new()).build();
```

- [ ] **Step 2: Run `cargo check`**

```
cargo check
```

Expected: zero errors, zero warnings about unused imports.

- [ ] **Step 3: Run `cargo build`**

```
cargo build
```

Expected: successful binary. Fix any remaining type errors.

- [ ] **Step 4: Commit**

```
git add src/main.rs
git commit -m "fix(main): align loop to new sync_queue signature and Queue struct"
```

---

## Verification Checklist

After all tasks complete:

- [ ] `cargo check` passes with zero errors
- [ ] `cargo build` produces a binary
- [ ] Smoke test — `NOVO` scenario:
  - Insert a row into `f_prgven` → trigger fires → `kronos_pdv_queue` gets a row with `sync=1`
  - Run the binary; confirm a new DealerCRM card is created with `FunctionalRequirements=order_code` and `BusinessRule=[sc_code]`
  - Confirm `sync=0` in the queue row after sync
- [ ] Smoke test — `ATUALIZAR` scenario:
  - Update a schedule in Oracle (e.g. change `prv_qtprog`) → `sync=1`
  - Run binary; confirm existing CRM card is PATCHed (detail updated), workflow column unchanged
- [ ] Smoke test — `EXCLUIR` (schedule deleted from order):
  - Delete a `f_prgven` row → trigger removes its `schedule_code` from the JSON array → `sync=1`
  - If the date group now has zero schedules, the CRM card is DELETEd
  - If other schedules remain on that date, the CRM card is PATCHed (business_rules updated)
- [ ] Smoke test — order deleted (`f_pedvenda` DELETE):
  - Trigger sets `schedules=[]`, `sync=1`
  - All CRM activities for that order are DELETEd
  - Queue row gets `sync=0`
