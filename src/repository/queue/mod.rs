use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sibyl::{Row, Session};
use tracing::error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub tipo: String,
    pub pedido: String,
    pub indice: i64,
    pub status: String,
    pub retries: i32,
    pub last_error: Option<String>,
    pub code: Option<i64>,
    pub guid: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl Card {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        let tipo: String = row.get(0)?.unwrap_or_default();
        let pedido: String = row.get(1)?.unwrap_or_default();
        let indice: i64 = row.get::<_, i64>(2)?.unwrap_or_default();
        let status: String = row.get(3)?.unwrap_or_default();
        let retries: i32 = row.get::<_, i32>(4)?.unwrap_or(0);
        let last_error: Option<String> = row.get(5)?;
        let code: Option<i64> = row.get(6)?;
        let guid: Option<String> = row.get(7)?;
        let created_at: Option<String> = row.get(8)?;
        let updated_at: Option<String> = row.get(9)?;

        Ok(Self {
            tipo,
            pedido,
            indice,
            status,
            retries,
            last_error,
            code,
            guid,
            created_at,
            updated_at,
        })
    }
}

pub async fn get_queue(session: &Session<'_>) -> Result<Vec<Card>> {
    let sql = "
        SELECT
            tipo,
            pedido,
            indice,
            status,
            retries,
            last_error,
            code,
            guid,
            to_char(created_at, 'YYYY-MM-DD HH24:MI:SS') AS created_at,
            to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS') AS updated_at
        FROM inventario.cards
        WHERE (status IN ('NOVO', 'ATUALIZAR', 'EXCLUIR')
           OR (status = 'TRAVADO' AND updated_at < SYSTIMESTAMP - INTERVAL '15' MINUTE))
          AND retries < 5
    ";

    let stmt = session.prepare(sql).await.context("Failed to prepare statement for get_queue")?;
    let rows = stmt.query(()).await.context("Failed to query queue from Oracle")?;
    let mut queue = Vec::new();

    while let Some(row) = rows.next().await? {
        let card = Card::from_row(&row)?;
        queue.push(card);
    }

    Ok(queue)
}

pub async fn update_status(
    session: &Session<'_>,
    status: &str,
    card: &Card,
) -> Result<()> {
    let sql = "
        UPDATE inventario.cards
        SET status = :1,
            updated_at = SYSTIMESTAMP
        WHERE tipo = :2
          AND pedido = :3
          AND indice = :4
    ";
    let stmt = session.prepare(sql).await.context("Failed to prepare statement for update_status")?;
    stmt.execute((status, &card.tipo, &card.pedido, card.indice)).await.context("Failed to execute update_status")?;
    session.commit().await.context("Failed to commit session after update_status")?;
    Ok(())
}

pub async fn update_code_guid(
    session: &Session<'_>,
    code: i64,
    guid: &str,
    card: &Card,
) -> Result<()> {
    let sql = "
        UPDATE inventario.cards
        SET code = :1,
            guid = :2,
            updated_at = SYSTIMESTAMP
        WHERE tipo = :3
          AND pedido = :4
          AND indice = :5
    ";
    let stmt = session.prepare(sql).await.context("Failed to prepare statement for update_code_guid")?;
    stmt.execute((code, guid, &card.tipo, &card.pedido, card.indice)).await.context("Failed to execute update_code_guid")?;
    session.commit().await.context("Failed to commit session after update_code_guid")?;
    Ok(())
}

pub async fn update_last_error(
    session: &Session<'_>,
    last_error: &str,
    card: &Card,
) -> Result<()> {
    let sql = "
        UPDATE inventario.cards
        SET last_error = :1,
            updated_at = SYSTIMESTAMP
        WHERE tipo = :2
          AND pedido = :3
          AND indice = :4
    ";
    let stmt = session.prepare(sql).await.context("Failed to prepare statement for update_last_error")?;
    stmt.execute((last_error, &card.tipo, &card.pedido, card.indice)).await.context("Failed to execute update_last_error")?;
    session.commit().await.context("Failed to commit session after update_last_error")?;
    Ok(())
}

pub async fn update_retries(
    session: &Session<'_>,
    card: &Card,
) -> Result<()> {
    let sql = "
        UPDATE inventario.cards
        SET retries = retries + 1,
            updated_at = SYSTIMESTAMP
        WHERE tipo = :1
          AND pedido = :2
          AND indice = :3
    ";
    let stmt = session.prepare(sql).await.context("Failed to prepare statement for update_retries")?;
    stmt.execute((&card.tipo, &card.pedido, card.indice)).await.context("Failed to execute update_retries")?;
    session.commit().await.context("Failed to commit session after update_retries")?;
    Ok(())
}
