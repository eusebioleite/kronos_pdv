use std::env::args;

use anyhow::Context;
use oracle::conn;
use tracing::info;

mod log;
mod config;
mod database;
mod api;
mod repository;

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    // inicializar logger
    info!("Setting up logger.");
    log::init();

    // inicializar configuração
    info!("Loading configuration.");
    let config = config::init();
    
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
        Ok(pool) => pool,
        Err(e) => {
            error!("Failed to build connection pool: {e}");
            anyhow::bail!("Failed to build connection pool: {e}");
        }
    };

    // inicializar rotina
    info!("Starting routine (Interval: {}s, Throttle: {}ms).", config.interval, config.throttle);
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(config.interval));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        info!("Tick: Fetching new cards from database...");
        let session = match pool.get_session().await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get session from pool: {}", e);
                continue;
            }
        };

        let queue = match repository::get_queue(&session).await {
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

        for card in queue {
            info!("Processing card pedido: {}, indice: {}", card.pedido, card.indice);
            
            match repository::process_card(&session, &card).await {
                Ok(_) => {
                    info!("Card {} processed successfully.", card.pedido);
                }
                Err(e) => {
                    error!("Error processing card {}: {}", card.pedido, e);
                    match repository::update_last_error(&session, &e.to_string(), &card).await {
                        Ok(_) => info!("Last error updated for card {}", card.pedido),
                        Err(e) => error!("Failed to update last error for card {}: {}", card.pedido, e),
                    }
                    match session.commit().await {
                        Ok(_) => info!("Session committed successfully."),
                        Err(e) => anyhow::bail!("Failed to commit session after processing card {}: {}", card.pedido, e),
                    }
                }
            }

            if config.throttle > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(config.throttle)).await;
            }
        }
        
        info!("Batch processing finished.");
    }
    
}
