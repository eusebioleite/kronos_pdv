use tracing::{error, info};
use std::sync::Arc;

mod api;
mod config;
mod database;
mod dealercrm;
mod log;
mod repository;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // inicializar logger
    info!("Setting up logger.");
    log::init();

    // inicializar configuração
    info!("Loading configuration.");
    let config = match config::init() {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load configuration: {e}");
            anyhow::bail!("Failed to load configuration: {e}");
        }
    };

    // inicializar banco de dados
    info!("Loading database.");
    let env = match database::get_oci_env() {
        Ok(env) => env,
        Err(e) => {
            error!("Failed to initialize OCI environment: {e}");
            anyhow::bail!("Failed to initialize OCI environment: {e}");
        }
    };

    let conn_info = match database::get_conn() {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to parse connection info: {e}");
            anyhow::bail!("Failed to parse connection info: {e}");
        }
    };

    let pool = match database::init_pool(env, &conn_info).await {
        Ok(pool) => Arc::new(pool),
        Err(e) => {
            error!("Failed to build connection pool: {e}");
            anyhow::bail!("Failed to build connection pool: {e}");
        }
    };

    let mysql_pool = match dealercrm::init_pool().await {
        Ok(pool) => pool,
        Err(e) => {
            error!("Failed to build CRM connection pool: {e}");
            anyhow::bail!("Failed to build CRM connection pool: {e}");
        }
    };

    let (error_tx, error_rx) = tokio::sync::mpsc::channel::<repository::queue::ErrorUpdate>(100);

    // Spawn error worker
    tokio::spawn(repository::queue::start_error_worker(pool.clone(), error_rx));

    // inicializar rotina
    info!(
        "Starting routine (Interval: {}s, Throttle: {}ms).",
        config.config.interval, config.config.throttle
    );
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(config.config.interval as u64));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        info!("Tick: Fetching new cards from database...");

        // Tenta obter uma sessão do pool de conexões.
        let session = match pool.get_session().await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get session from pool: {}", e);
                continue;
            }
        };

        // Tenta fazer um select na fila.
        let queue = match repository::queue::get_queue(&session).await {
            Ok(q) => q,
            Err(e) => {
                error!("Failed to fetch queue: {}", e);
                continue;
            }
        };

        if queue.is_empty() {
            info!("No cards to process. Waiting for next tick.");
            continue;
        }

        info!("Found {} cards to process.", queue.len());

        // Processa cada card encontrado.
        for card in queue {
            info!("Processing card pedido: {}", card.pedido);

            match repository::sync::sync_queue(&session, &card, &config.config, &mysql_pool).await {
                Ok(_) => {
                    info!("Card {} processed successfully.", card.pedido);
                }
                Err(e) => {
                    error!("Error processing card {}: {}", card.pedido, e);
                    if let Err(err) = error_tx.send(repository::queue::ErrorUpdate {
                        pedido: card.pedido.clone(),
                        last_error: format!("{:?}", e),
                    }).await {
                        error!("Failed to send error update to worker: {}", err);
                    }
                }
            }

            if config.config.throttle > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(config.config.throttle as u64)).await;
            }
        }

        info!("Batch processing finished.");
    }
}
