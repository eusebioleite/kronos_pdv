use anyhow::{Context, Result};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{FromRow, MySqlPool};
use std::sync::OnceLock;

#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
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

/// Returns a reference to the global MySQL connection pool.
/// Panics if called before `dealercrm::init_pool()`.
pub fn get_pool() -> &'static MySqlPool {
    MYSQL_POOL
        .get()
        .expect("dealercrm::init_pool() must be called before get_pool()")
}

pub async fn init_pool() -> Result<()> {
    let cfg = &crate::config::get().config.mysql;
    let url = format!(
        "mysql://{}:{}@{}:{}/{}",
        urlencoding::encode(&cfg.user),
        urlencoding::encode(&cfg.password),
        cfg.host,
        cfg.port,
        cfg.database,
    );

    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .context("Failed to connect to MySQL DealerCRM")?;
    MYSQL_POOL
        .set(pool)
        .map_err(|_| anyhow::anyhow!("dealercrm::init_pool() was called more than once"))?;
    Ok(())
}

pub async fn fetch_activities(order_code: &str) -> Result<Vec<Activity>> {
    let pool = get_pool();
    let query_str = "
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
        JOIN WorkflowStages ws          ON aws.ActivityWorkflowStages_WorkflowStagesCode = ws.WorkflowStages_Code 
        WHERE a.Activity_FunctionalRequirements = ?
          AND a.Activity_TupleExcluded = 0
    ";

    let activities = sqlx::query_as::<_, Activity>(query_str)
        .bind(order_code)
        .fetch_all(pool)
        .await
        .with_context(|| format!("Failed to fetch activities for order_code '{order_code}'"))?;

    Ok(activities)
}
