use anyhow::{Context, Result};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{FromRow, MySqlPool};
use std::sync::OnceLock;

#[derive(Debug, Clone, FromRow)]
pub struct ActivityRow {
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
    #[sqlx(rename = "Activity_Problem")]
    pub activity_problem: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Activity {
    pub activity_code: i64,
    pub activity_guid: String,
    pub activity_title: Option<String>,
    pub activity_detail: Option<String>,
    pub activity_functional_requirements: Option<String>,
    pub activity_business_rule: Option<String>,
    pub activity_planned_date: Option<chrono::NaiveDate>,
    pub activity_problem: Option<String>,
    pub chats: Vec<Chat>,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
pub struct Chat {
    #[sqlx(rename = "ActivityChat_Code")]
    pub activity_chat_code: i64,
    #[sqlx(rename = "ActivityChat_Guid")]
    pub activity_chat_guid: String,
    #[sqlx(rename = "Activity_Code")]
    pub activity_code: i64,
    #[sqlx(rename = "ActivityChat_PersonCode")]
    pub activity_chat_person_code: i64,
    #[sqlx(rename = "ActivityChat_Text")]
    pub activity_chat_text: String,
    #[sqlx(rename = "ActivityChat_CommentDate")]
    pub activity_chat_comment_date: chrono::NaiveDateTime,
}

#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
pub struct Attachment {
    #[sqlx(rename = "ActivityAttachment_Code")]
    pub activity_attachment_code: i64,
    #[sqlx(rename = "ActivityAttachment_Guid")]
    pub activity_attachment_guid: String,
    #[sqlx(rename = "Activity_Code")]
    pub activity_code: i64,
    #[sqlx(rename = "ActivityAttachment_Description")]
    pub activity_attachment_description: String,
}

static MYSQL_POOL: OnceLock<MySqlPool> = OnceLock::new();

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
    let activity_query = "
        SELECT DISTINCT
            a.Activity_Code, 
            a.Activity_Guid,
            a.Activity_Title,
            a.Activity_Detail,  
            a.Activity_Problem,  
            a.Activity_FunctionalRequirements, 
            a.Activity_BusinessRule, 
            a.Activity_PlannedDate
        FROM Activity a
        JOIN ActivityWorkflowStages aws ON aws.ActivityWorkflowStages_ActivityCode = a.Activity_Code 
        JOIN WorkflowStages ws          ON aws.ActivityWorkflowStages_WorkflowStagesCode = ws.WorkflowStages_Code 
        WHERE a.Activity_FunctionalRequirements = ?
          AND a.Activity_TupleExcluded = 0
    ";

    let activities: Vec<ActivityRow> = sqlx::query_as::<_, ActivityRow>(activity_query)
        .bind(order_code)
        .fetch_all(pool)
        .await
        .with_context(|| format!("Failed to fetch activities for order_code '{order_code}'"))?;

    let mut activities: Vec<Activity> = activities
        .iter()
        .map(|activity| Activity {
            activity_code: activity.activity_code,
            activity_guid: activity.activity_guid.clone(),
            activity_title: activity.activity_title.clone(),
            activity_detail: activity.activity_detail.clone(),
            activity_problem: activity.activity_problem.clone(),
            activity_functional_requirements: activity.activity_functional_requirements.clone(),
            activity_business_rule: activity.activity_business_rule.clone(),
            activity_planned_date: activity.activity_planned_date,
            chats: vec![],
            attachments: vec![],
        })
        .collect();

    for activity in activities.iter_mut() {
        let chat_query = "
        SELECT
            ActivityChat_Code,
            ActivityChat_Guid,
            Activity_Code,
            ActivityChat_PersonCode,
            ActivityChat_Text,
            ActivityChat_CommentDate
        FROM ActivityChat
        WHERE Activity_Code = ?
          AND ActivityChat_TupleExcluded = 0
        ";
        let chats: Vec<Chat> = sqlx::query_as::<_, Chat>(chat_query)
            .bind(activity.activity_code)
            .fetch_all(pool)
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch chats for activity_code '{}'",
                    activity.activity_code
                )
            })?;
        activity.chats = chats;

        let attachment_query = "
        SELECT
            ActivityAttachment_Code,
            ActivityAttachment_Guid,
            Activity_Code,
            ActivityAttachment_Description
        FROM ActivityAttachment
        WHERE Activity_Code = ?
          AND ActivityAttachment_TupleExcluded = 0
        ";
        let attachments: Vec<Attachment> = sqlx::query_as::<_, Attachment>(attachment_query)
            .bind(activity.activity_code)
            .fetch_all(pool)
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch attachments for activity_code '{}'",
                    activity.activity_code
                )
            })?;
        activity.attachments = attachments;
    }

    Ok(activities)
}
