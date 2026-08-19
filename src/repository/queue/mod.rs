use anyhow::{Context, Result};
use sibyl::Row;

use crate::database;

#[derive(Debug, Clone)]
#[allow(dead_code)]
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

        Ok(Self {
            order_code,
            retries,
            error,
        })
    }
}

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
        ORDER BY updated_at
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare get_queue statement for kronos_pdv_queue")?;

    let rows = stmt
        .query(())
        .await
        .context("Failed to query queue records from Oracle kronos_pdv_queue")?;

    let mut queue = Vec::new();

    while let Some(row) = rows
        .next()
        .await
        .context("Failed to fetch next row from queue query")?
    {
        let card = Queue::from_row(&row).context("Failed to parse row into Queue model")?;
        queue.push(card);
    }

    Ok(queue)
}

pub async fn mark_sync(order_code: &str) -> Result<()> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get session from pool.")?;

    let sql = "
        UPDATE kronos_pdv_queue
        SET 
            sync = 0,
            retries = 0,
            updated_at = SYSTIMESTAMP
        WHERE order_code = :1
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for mark_synced on kronos_pdv_queue")?;

    stmt.execute(order_code)
        .await
        .with_context(|| format!("Failed to execute mark_synced for order_code '{order_code}'"))?;

    session.commit().await.with_context(|| {
        format!("Failed to commit session after mark_synced for order_code '{order_code}'")
    })?;

    Ok(())
}

pub async fn update_error(order_code: &str, last_error: &str) -> Result<()> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get session from pool.")?;

    let mut error_msg = last_error.to_string();
    if error_msg.len() > 3990 {
        error_msg.truncate(3990);
        error_msg.push_str("...");
    }

    let sql = "
        UPDATE kronos_pdv_queue
        SET 
            error = json_transform(COALESCE(error, '[]'), APPEND '$' = json_scalar(:1)),
            retries = retries + 1,
            updated_at = SYSTIMESTAMP
        WHERE order_code = :2
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for update_error on kronos_pdv_queue")?;

    stmt.execute((error_msg.as_str(), order_code))
        .await
        .with_context(|| format!("Failed to execute update_error for order_code '{order_code}'"))?;

    session.commit().await.with_context(|| {
        format!("Failed to commit session after update_error for order_code '{order_code}'")
    })?;

    Ok(())
}
