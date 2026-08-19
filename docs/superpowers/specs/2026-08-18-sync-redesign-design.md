# Kronos PDV — Sync Redesign (Set Reconciliation)

**Date:** 2026-08-18  
**Status:** Approved for implementation

---

## Problem

The previous design used a status-based queue (`NOVO`, `ATUALIZAR`, `EXCLUIR`, `TRAVADO`) with separate branching logic per status and per order kind (`PDV-A`/`PDV-F`). This created complexity, edge cases, and made the sync unreliable under concurrent ERP changes.

## Solution

Replace the status machine with a **pure set-reconciliation model**. The Oracle queue holds one row per `order_code` with a `schedules JSON` array representing what _should_ exist. On each sync tick, Rust fetches Oracle's current ground truth, fetches DealerCRM's current state, and applies the minimal diff (POST / PATCH / DELETE) to make CRM match ERP.

---

## Oracle Layer (already implemented)

### `kronos_pdv_queue` schema

```sql
create table kronos_pdv_queue (
    order_code  varchar2(40)  not null,
    schedules   json,                         -- array of active schedule_codes
    sync        number(1) default 0 not null, -- 1 = needs sync, 0 = up to date
    retries     number(1) default 0 not null,
    last_error  json,
    created_at  timestamp(6) default systimestamp not null,
    updated_at  timestamp(6) default systimestamp not null,
    constraint pk_kronos_pdv_queue primary key (order_code),
    constraint chk_kronos_pdv_sync check (sync in (0, 1)),
    constraint chk_kronos_pdv_retries check (retries between 0 and 5)
);
```

**One row per `order_code`.** The `schedules` array is maintained by triggers.

### Trigger invariants

| Oracle event | Trigger effect on queue |
|---|---|
| `f_prgven INSERT/UPDATE` | Adds `prv_indice` to `schedules[]` (no dup); sets `sync=1` |
| `f_prgven DELETE` | Removes `prv_indice` from `schedules[]`; sets `sync=1` |
| `f_pedvenda DELETE` | Sets `schedules=[]`; sets `sync=1` |
| `f_pedvenda INSERT/UPDATE` (status=A) | Rebuilds `schedules[]` from all `f_prgven` rows; sets `sync=1` |

When `schedules=[]` and `sync=1`, Rust will find zero new_cards and DELETE all CRM activities for that order — correct behavior.

---

## Rust Layer

### Data flow

```
main loop
  └── get_queue()                    → Vec<Queue>             (Oracle: WHERE sync=1)
        └── for each item:
              sync_queue(order_code)
                ├── get_new_cards()  → Vec<ActivityComplete>  (Oracle, grouped by date)
                ├── get_activities() → Vec<Activity>          (MySQL DealerCRM)
                ├── diff → POST / PATCH / DELETE
                └── mark_synced()   → UPDATE sync=0
```

### Structs

#### `Queue` (src/repository/queue/mod.rs)

```rust
pub struct Queue {
    pub order_code: String,
    pub retries: i32,
    pub error: Option<String>,
}
```

> `schedules` is NOT needed in Rust. The Oracle query in `get_new_cards` fetches all
> current schedules fresh on each sync. The queue's `schedules` column is trigger bookkeeping only.

#### `Schedule` (src/repository/sync/mod.rs) — replaces `Order`

One row from Oracle's `f_prgven JOIN f_pedvenda JOIN ...`:

```rust
pub struct Schedule {
    pub order_code: String,
    pub schedule_code: i64,
    pub product_code: String,
    pub product_description: String,
    pub schedule_qtd: f64,
    pub product_bottle: f64,
    pub schedule_date: chrono::NaiveDate,
    pub company_code: i32,
    pub product_type: i32,   // 1-7 from CASE expression
    pub order_type: String,  // VENDA / AMOSTRA / TRANSF / OUTROS
    pub delivery_type: String,
    pub order_nature: String,
    pub nature_description: String,
    pub order_seller: String,
    pub customer_code: i32,
    pub customer_name: String,
    pub customer_fantasy: String,
}
```

#### `ActivityCreate` (src/api/mod.rs) — POST payload

```rust
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
    pub functional_requirements: String,  // = order_code
    pub business_rules: Vec<i64>,          // = [sc1, sc2, ...]
    pub workflow_stages: Vec<WorkflowStage>,
}
```

#### `ActivityUpdate` (src/api/mod.rs) — PATCH payload (no workflow_stages)

```rust
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

#### `WorkflowStage` (src/api/mod.rs)

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStage {
    pub workflow_stages_code: i64,
    pub code: i64,
    pub order: i64,
    pub order_to: i64,
}
```

Values come from `Config.default_stage_code`.

#### `Activity` (src/dealercrm/mod.rs) — simplified

```rust
#[derive(Debug, Clone, FromRow)]
pub struct Activity {
    pub activity_code: i64,
    pub activity_guid: String,
    pub activity_title: Option<String>,
    pub activity_detail: Option<String>,
    pub activity_functional_requirements: Option<String>,
    pub activity_business_rule: Option<String>,  // JSON stored as string e.g. "[1,2,3]"
    pub activity_planned_date: Option<chrono::NaiveDate>,
}
```

---

### `get_new_cards(order_code: &str)` → `Vec<ActivityComplete>`

1. Query Oracle: `SELECT ... FROM f_prgven JOIN f_pedvenda WHERE pdv_numped = :1`
   (no `prv_indice` filter — gets ALL schedules for the order)
2. Deserialize each row into `Schedule`
3. Group by `schedule_date` using `BTreeMap<NaiveDate, Vec<Schedule>>` (BTreeMap = sorted by date)
4. For each date group, build one `ActivityComplete`:
   - `title` = `format!("PDV {} | {}", order_code.trim_start_matches('0'), customer_name)`
   - `detail` = `build_detail(&group)` — HTML bullet per Schedule in the group
   - `type_activity_code` = `get_activity_code(&group[0])` (first row representative for the group)
   - `planned_date` / `replanned_date` = the group's date
   - `objective` = `delivery_type` from first schedule
   - `script` = `customer_fantasy` from first schedule
   - `functional_requirements` = `order_code`
   - `business_rules` = `group.iter().map(|s| s.schedule_code).collect()`

### `get_activities(order_code: &str)` → `Vec<Activity>`

```sql
SELECT
    Activity_Code,
    Activity_Guid,
    Activity_Title,
    Activity_Detail,
    Activity_FunctionalRequirements,
    Activity_BusinessRule,
    Activity_PlannedDate
FROM Activity
JOIN ActivityWorkflowStages ON ActivityWorkflowStages_ActivityCode = Activity_Code
JOIN WorkflowStages ON ActivityWorkflowStages_WorkflowStagesCode = WorkflowStages_Code
WHERE Activity_FunctionalRequirements = ?
```

### `sync_queue(order_code)` — the diff algorithm

```rust
let new_cards  = get_new_cards(order_code).await?;
let activities = get_activities(order_code).await?;

// Key both maps by date
let activities_map: HashMap<NaiveDate, Activity> = activities
    .into_iter()
    .filter_map(|a| a.activity_planned_date.map(|d| (d, a)))
    .collect();

let new_cards_map: HashMap<NaiveDate, ActivityComplete> = new_cards
    .into_iter()
    .map(|c| (c.planned_date, c))
    .collect();

// POST: date exists in Oracle but not in CRM
for (date, new_card) in &new_cards_map {
    if !activities_map.contains_key(date) {
        api::new_card(client, config, &new_card.as_create(config)).await?;
    }
}

// PATCH: date exists in both — Oracle overwrites CRM
for (date, new_card) in &new_cards_map {
    if let Some(existing) = activities_map.get(date) {
        api::update_card(client, config, existing.activity_code, &new_card.as_update()).await?;
    }
}

// DELETE: date in CRM but not in Oracle — remove by GUID
for (date, obsolete) in &activities_map {
    if !new_cards_map.contains_key(date) {
        api::delete_card(client, config, &obsolete.activity_guid).await?;
    }
}

mark_synced(order_code).await?;
```

---

### `build_detail(schedules: &[Schedule])` → String

Produces concatenated HTML, one block per Schedule:

```
• <strong>📋 Índice:</strong> <em>{schedule_code}</em><br>
• <strong>📦 Produto:</strong> <em>{product_code} | {product_description}</em><br>
• <strong>🔢 Quantidade:</strong> <em>N volumes de X unidades (Y total)</em><br><br>
```

Quantity formatting rules:
- `product_bottle == 0.0` → `"{qtd} unidades no total"`
- `boxes > 0 && surplus > 0` → `"{boxes} volumes de {bottle} + 1 volume com {surplus} ({qtd} total)"`
- `boxes > 0 && surplus == 0` → `"{boxes} volumes de {bottle} unidades ({qtd} total)"`
- `boxes == 0` → `"1 volume com {qtd} unidades no total"`

---

### Queue lifecycle

| Rust event | Oracle update |
|---|---|
| Sync succeeds | `UPDATE SET sync=0` (trigger will flip to 1 again on next ERP change) |
| Sync fails | `UPDATE SET last_error=..., retries=retries+1` (`sync` stays 1) |
| `retries >= 5` | Row excluded by `WHERE retries < 5` — manual intervention needed |

---

## Design invariants

- **Oracle is the single source of truth.** CRM always follows ERP.
- **`planned_date` is locked in CRM.** Changes must happen in the ERP.
- **PATCH never sends `workflow_stages`.** Preserves CRM's workflow column.
- **DELETE uses GUID**, not `Activity_Code`.
- **`Activity_FunctionalRequirements`** = `order_code`
- **`Activity_BusinessRule`** = JSON array of `schedule_codes` (`Vec<i64>` serialized)
- No status branching. No order_kind branching. One sync function, one diff algorithm.
