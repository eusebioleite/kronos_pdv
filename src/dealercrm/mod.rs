use anyhow::{Context, Result};
use serde::Serialize;
use serde::ser::SerializeStruct;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{FromRow, MySqlPool};

#[derive(Debug, Clone, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub activity_code: i64,
    pub activity_guid: String,
    pub activity_script: Option<String>,
    pub activity_title: Option<String>,
    pub activity_problem: Option<String>,
    pub activity_objective: Option<String>,
    pub activity_detail: Option<String>,
    pub activity_functional_requirements: Option<String>,
    pub activity_business_rule: Option<String>,
    pub title: Option<String>,
    pub activity_planned_date: Option<chr9o>,
    pub activity_replanned_date: Option<String>,
    pub activity_workflow_stages_code: i64,
    pub workflow_stages_code: i64,
    pub workflow_stages_description: Option<String>,
}

pub async fn init_pool() -> Result<MySqlPool> {
    MySqlPoolOptions::new()
        .max_connections(5)
        .connect("mysql://dealercrm:123456@localhost:3306/dealercrm")
        .await
        .context("Failed to connect to MySQL DealerCRM")
}

pub async fn fetch_activities(
    pool: &MySqlPool,
    order_code: &str,
    schedule_code: i64,
) -> Result<Vec<Activity>> {
    let query_str = "
        select
            Activity_Code, 
            Activity_Guid,
            Activity_Title,
            Activity_Detail,  
            Activity_Problem,
            Activity_Script,
            Activity_Objective,
            Activity_FunctionalRequirements, 
            Activity_BusinessRule, 
            Activity_PlannedDate, 
            Activity_ReplannedDate,
            ActivityWorkflowStages_Code,
            WorkflowStages_Code,
            WorkflowStages_Description
        from Activity 
        join ActivityWorkflowStages on ActivityWorkflowStages_ActivityCode       = Activity_Code 
        join WorkflowStages         on ActivityWorkflowStages_WorkflowStagesCode = WorkflowStages_Code 
        WHERE ActivityFunctionalRequirements = ?
        AND Activity_BusinessRule = ?
    ";

    let activities = sqlx::query_as::<_, Activity>(query_str)
        .bind((order_code, schedule_code))
        .fetch_all(pool)
        .await
        .with_context(|| format!("Failed to fetch activities for order_code {order_code} and schedule_code {schedule_code}"))?;

    Ok(activities)
}
