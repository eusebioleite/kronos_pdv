use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use sibyl::{Row, Session};
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Queue {
    pub pedido: String,
    pub status: String,
    pub retries: i32,
    pub last_error: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ErrorUpdate {
    pub pedido: String,
    pub last_error: String,
}

impl Queue {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        let pedido: String = match row.get::<Option<String>, _>(0usize)? {
            Some(s) => s,
            None => String::new(),
        };
        let status: String = match row.get::<Option<String>, _>(1usize)? {
            Some(s) => s,
            None => String::new(),
        };
        let retries: i32 = match row.get::<Option<i32>, _>(2usize)? {
            Some(r) => r,
            None => 0,
        };
        let last_error: Option<String> = row.get(3usize)?;
        let created_at: Option<String> = row.get(4usize)?;
        let updated_at: Option<String> = row.get(5usize)?;

        Ok(Self {
            pedido,
            status,
            retries,
            last_error,
            created_at,
            updated_at,
        })
    }
}

pub async fn get_queue(session: &Session<'_>) -> Result<Vec<Queue>> {
    let sql = "
        SELECT
            pedido,
            status,
            retries,
            last_error,
            to_char(created_at, 'YYYY-MM-DD HH24:MI:SS') AS created_at,
            to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS') AS updated_at
        FROM inventario.cards
        WHERE (status IN ('NOVO', 'ATUALIZAR', 'EXCLUIR')
        OR (status = 'TRAVADO' AND updated_at < SYSTIMESTAMP - INTERVAL '15' MINUTE))
        AND retries < 5
    ";

    let stmt = match session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for get_queue")
    {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for get_queue: {}", e);
            return Err(anyhow!("Failed to prepare statement for get_queue: {}", e));
        }
    };
    let rows = match stmt
        .query(())
        .await
        .context("Failed to query queue from Oracle")
    {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to query queue from Oracle: {}", e);
            return Err(anyhow!("Failed to query queue from Oracle: {}", e));
        }
    };

    let mut queue = Vec::new();

    while let Some(row) = match rows.next().await {
        Ok(Some(r)) => Some(r),
        Ok(None) => None,
        Err(e) => {
            error!("Failed to fetch row from queue: {}", e);
            return Err(anyhow!("Failed to fetch row from queue: {}", e));
        }
    } {
        let card = match Queue::from_row(&row) {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to parse row into Queue: {}", e);
                return Err(anyhow!("Failed to parse row into Queue: {}", e));
            }
        };

        queue.push(card);
    }

    Ok(queue)
}

pub async fn update_status(session: &Session<'_>, status: &str, queue: &Queue) -> Result<()> {
    let sql = "
        UPDATE inventario.cards
        SET 
            status = :1,
            updated_at = SYSTIMESTAMP
        WHERE pedido = :2
    ";

    let stmt = match session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for update_status")
    {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for update_status: {}", e);
            return Err(anyhow!(
                "Failed to prepare statement for update_status: {}",
                e
            ));
        }
    };

    match stmt
        .execute((status, &queue.pedido))
        .await
        .context("Failed to execute update_status")
    {
        Ok(_) => {}
        Err(e) => {
            error!("Failed to execute update_status: {}", e);
            return Err(anyhow!("Failed to execute update_status: {}", e));
        }
    }

    match session
        .commit()
        .await
        .context("Failed to commit session after update_status")
    {
        Ok(_) => {}
        Err(e) => {
            error!("Failed to commit session after update_status: {}", e);
            return Err(anyhow!(
                "Failed to commit session after update_status: {}",
                e
            ));
        }
    }

    Ok(())
}

pub async fn start_error_worker(
    pool: std::sync::Arc<sibyl::SessionPool<'static>>,
    mut rx: tokio::sync::mpsc::Receiver<ErrorUpdate>,
) {
    while let Some(update) = rx.recv().await {
        let session = match pool.get_session().await {
            Ok(s) => s,
            Err(e) => {
                error!("Worker: Failed to get session from pool for error update: {:?}", e);
                continue;
            }
        };

        let mut error_msg = update.last_error;
        if error_msg.len() > 32000 {
            error_msg.truncate(32000);
        }

        let sql = "
            UPDATE inventario.cards
            SET 
                last_error = :1,
                retries = retries + 1,
                updated_at = SYSTIMESTAMP
            WHERE pedido = :2
        ";

        let stmt = match session.prepare(sql).await {
            Ok(s) => s,
            Err(e) => {
                error!("Worker: Failed to prepare statement for update_last_error: {:?}", e);
                continue;
            }
        };

        if let Err(e) = stmt.execute((error_msg.as_str(), update.pedido.as_str())).await {
            error!("Worker: Failed to execute update_last_error: {:?}", e);
            continue;
        }

        if let Err(e) = session.commit().await {
            error!("Worker: Failed to commit session after update_last_error: {:?}", e);
            continue;
        }

        info!("Worker: Last error updated for card {}", update.pedido);
    }
}

#[allow(dead_code)]
pub async fn update_retries(session: &Session<'_>, queue: &Queue) -> Result<()> {
    let sql = "
        UPDATE inventario.cards
        SET retries = retries + 1,
            updated_at = SYSTIMESTAMP
        WHERE pedido = :1
    ";

    let stmt = match session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for update_retries")
    {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for update_retries: {}", e);
            return Err(anyhow!(
                "Failed to prepare statement for update_retries: {}",
                e
            ));
        }
    };

    match stmt
        .execute(&queue.pedido)
        .await
        .context("Failed to execute update_retries")
    {
        Ok(_) => {}
        Err(e) => {
            error!("Failed to execute update_retries: {}", e);
            return Err(anyhow!("Failed to execute update_retries: {}", e));
        }
    }

    match session
        .commit()
        .await
        .context("Failed to commit session after update_retries")
    {
        Ok(_) => {}
        Err(e) => {
            error!("Failed to commit session after update_retries: {}", e);
            return Err(anyhow!(
                "Failed to commit session after update_retries: {}",
                e
            ));
        }
    }

    Ok(())
}
