use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sibyl::{Row, Session};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Queue {
    pub pedido: String,
    pub status: String,
    pub retries: i32,
    pub last_error: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl Queue {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        let pedido: String = row
            .get::<Option<String>, _>(0usize)
            .context("Failed to read 'pedido' (column 0) from row")?
            .unwrap_or_default();
        let status: String = row
            .get::<Option<String>, _>(1usize)
            .context("Failed to read 'status' (column 1) from row")?
            .unwrap_or_default();
        let retries: i32 = row
            .get::<Option<i32>, _>(2usize)
            .context("Failed to read 'retries' (column 2) from row")?
            .unwrap_or_default();
        let last_error: Option<String> = row
            .get(3usize)
            .context("Failed to read 'last_error' (column 3) from row")?;
        let created_at: Option<String> = row
            .get(4usize)
            .context("Failed to read 'created_at' (column 4) from row")?;
        let updated_at: Option<String> = row
            .get(5usize)
            .context("Failed to read 'updated_at' (column 5) from row")?;

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

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for get_queue from inventario.cards")?;

    let rows = stmt
        .query(())
        .await
        .context("Failed to query queue records from Oracle inventario.cards")?;

    let mut queue = Vec::new();

    while let Some(row) = rows
        .next()
        .await
        .context("Failed to fetch next row from queue query")?
    {
        let card = Queue::from_row(&row)
            .context("Failed to parse row into Queue model")?;

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

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for update_status on inventario.cards")?;

    stmt.execute((status, &queue.pedido))
        .await
        .with_context(|| {
            format!(
                "Failed to execute update_status to '{}' for pedido '{}'",
                status, queue.pedido
            )
        })?;

    session
        .commit()
        .await
        .with_context(|| {
            format!(
                "Failed to commit session after update_status to '{}' for pedido '{}'",
                status, queue.pedido
            )
        })?;

    Ok(())
}

pub async fn update_last_error(
    session: &Session<'_>,
    last_error: &str,
    queue: &Queue,
) -> Result<()> {
    let mut error_msg = last_error.to_string();
    if error_msg.len() > 4000 {
        error_msg.truncate(4000);
    }

    let sql = "
        UPDATE inventario.cards
        SET 
            last_error = :1,
            retries = retries + 1,
            updated_at = SYSTIMESTAMP
        WHERE pedido = :2
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for update_last_error on inventario.cards")?;

    stmt.execute((error_msg.as_str(), queue.pedido.as_str()))
        .await
        .with_context(|| {
            format!(
                "Failed to execute update_last_error for pedido '{}'",
                queue.pedido
            )
        })?;

    session
        .commit()
        .await
        .with_context(|| {
            format!(
                "Failed to commit session after update_last_error for pedido '{}'",
                queue.pedido
            )
        })?;

    Ok(())
}
