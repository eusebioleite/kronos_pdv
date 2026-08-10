use anyhow::Result;
use serde::{Deserialize, Serialize};
use sibyl::Session;
use sqlx::any;

use crate::repository::sync::Order;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub tipo: String,
    pub pedido: i64,
    pub indice: i64,
    pub status: String,
    pub retries: i32,
    pub last_error: Option<String>,
    pub code: Option<String>,
    pub guid: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl Card {
    pub fn from_row(row: &Row) -> Result<Self, Error> {
        Ok(Self {
            tipo: row.get(0)?,
            pedido: row.get(1)?,
            indice: row.get(2)?,
            status: row.get(3)?,
            retries: row.get(4)?,
            last_error: row.get(5)?,
            code: row.get(6)?,
            guid: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    }
}

pub async fn get_queue(session: &Session<'_>) -> Result<Vec<Card>, anyhow::Error> {
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
    let stmt = match session.prepare(sql).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for getting queue: {}", e);
            anyhow::bail!("Failed to prepare statement for getting queue: {}", e);
        }
    
    };
    let rows = stmt.query(()).await?;
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
) -> Result<(), anyhow::Error> {
    let sql = "
        UPDATE inventario.cards
        SET status = :1,
            updated_at = SYSTIMESTAMP
        WHERE tipo = :2
          AND pedido = :3
          AND indice = :4
    ";
    let stmt = match session.prepare(sql).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for updating status: {}", e);
            anyhow::bail!("Failed to execute statement for updating status: {}", e);
        }
    
    };
    let _ = match stmt.execute((status, &card.tipo, &card.pedido, &card.indice)).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Failed to execute statement for updating status: {}", e);
            anyhow::bail!("Failed to execute statement for updating status: {}", e);
        }
    };
    Ok(())
}

pub async fn update_code_guid(
    session: &Session<'_>,
    code: &str,
    guid: &str,
    card: &Card,
) -> Result<(), anyhow::Error> {
    let sql = "
        UPDATE inventario.cards
        SET code = :1,
            guid = :2,
            updated_at = SYSTIMESTAMP
        WHERE tipo = :3
          AND pedido = :4
          AND indice = :5
    ";
    let stmt = match session.prepare(sql).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for updating code and guid: {}", e);
            anyhow::bail!("Failed to execute statement for updating code and guid: {}", e);
        }
    
    };
    let _ = match stmt.execute((code, guid, &card.tipo, &card.pedido, &card.indice)).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Failed to execute statement for updating code and guid: {}", e);
            anyhow::bail!("Failed to execute statement for updating code and guid: {}", e);
        }
    };

    Ok(())
}

pub async fn update_last_error(
    session: &Session<'_>,
    last_error: &str,
    card: &Card,
) -> Result<(), anyhow::Error> {
    let sql = "
        UPDATE inventario.cards
        SET last_error = :1,
            updated_at = SYSTIMESTAMP
        WHERE tipo = :2
          AND pedido = :3
          AND indice = :4
    ";
    
    let stmt = match session.prepare(sql).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for updating last_error: {}", e);
            anyhow::bail!("Failed to execute statement for updating last_error: {}", e);
        }
    
    };

    let _ = match stmt.execute((last_error, &card.tipo, &card.pedido, &card.indice)).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Failed to execute statement for updating last_error: {}", e);
            anyhow::bail!("Failed to execute statement for updating last_error: {}", e);
        }
    };
    Ok(())
}

pub async fn update_retries(
    session: &Session<'_>,
    card: &Card,
) -> Result<(), anyhow::Error> {
    let sql = "
        UPDATE inventario.cards
        SET retries = retries + 1,
            updated_at = SYSTIMESTAMP
        WHERE tipo = :1
          AND pedido = :2
          AND indice = :3
    ";
    
    let stmt = match session.prepare(sql).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for updating retries: {}", e);
            anyhow::bail!("Failed to execute statement for updating retries: {}", e);
        }
    };

    let _ = match stmt.execute((&card.tipo, &card.pedido, &card.indice)).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Failed to execute statement for updating retries: {}", e);
            anyhow::bail!("Failed to execute statement for updating retries: {}", e);
        }
    };
    Ok(())
}

pub async fn process_card(session: &Session<'_>, item: &Card) -> Result<(), anyhow::Error> {
    todo!();
}