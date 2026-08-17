use anyhow::{Context, Result, anyhow};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{MySqlPool, FromRow};
use tracing::error;

#[derive(Debug, Clone, FromRow)]
pub struct Activity {
    #[sqlx(rename = "ActivityCode")]
    pub code: i64,
    #[sqlx(rename = "ActivityFunctionalRequirements")]
    pub functional_requirements: Option<String>,
    #[sqlx(rename = "ActivityTitle")]
    pub title: Option<String>,
    #[sqlx(rename = "ActivityPlannedDate")]
    pub planned_date: Option<String>,
    #[sqlx(rename = "ActivityDetail")]
    pub detail: Option<String>,
    #[sqlx(rename = "ActivityClosed")]
    pub activity_closed: Option<i32>,
    #[sqlx(rename = "ActivityArchived")]
    pub activity_archived: Option<i32>,
}

pub async fn init_pool() -> Result<MySqlPool> {
    MySqlPoolOptions::new()
        .max_connections(5)
        .connect("mysql://dealercrm:123456@localhost:3306/dealercrm")
        .await
        .context("Failed to connect to MySQL DealerCRM")
}

pub async fn fetch_activities(pool: &MySqlPool, order_code: &str) -> Result<Vec<Activity>> {
    let query_str = "
        SELECT 
            ActivityCode, 
            ActivityFunctionalRequirements, 
            ActivityTitle, 
            ActivityPlannedDate, 
            ActivityDetail, 
            ActivityClosed, 
            ActivityArchived 
        FROM Activity 
        WHERE ActivityFunctionalRequirements LIKE ?
    ";
    
    let pattern = format!("%{}%", order_code);

    let activities = sqlx::query_as::<_, Activity>(query_str)
        .bind(pattern)
        .fetch_all(pool)
        .await
        .context(format!("Failed to fetch activities for order {}", order_code))?;

    Ok(activities)
}
