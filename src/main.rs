use anyhow::Context;
use tracing::{error, info};

mod api;
mod config;
mod database;
mod dealercrm;
mod log;
mod repository;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // inicializar logger
    let _log_guard = log::init();
    info!("Setting up logger.");

    // inicializar configuração
    info!("Loading configuration.");
    config::init().context("Failed to load application configuration from file")?;

    // inicializar banco de dados
    info!("Loading database.");
    database::init_pool()
        .await
        .context("Failed to build Oracle connection pool")?;

    dealercrm::init_pool()
        .await
        .context("Failed to build MySQL CRM connection pool")?;

    api::auth::init();

    // inicializar rotina
    info!(
        "Starting routine (Interval: {}s, Throttle: {}ms).",
        config::get().config.interval,
        config::get().config.throttle
    );
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
        config::get().config.interval as u64,
    ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        // Tenta fazer um select na fila.
        let queue = match repository::queue::get_queue().await {
            Ok(q) => q,
            Err(e) => {
                error!("Failed to fetch queue: {}", e);
                continue;
            }
        };

        if queue.is_empty() {
            continue;
        }

        info!("Found {} items to process.", queue.len());

        // Processa cada card encontrado.
        for item in queue {
            info!("Processing item: {}", item.order_code);

            match repository::sync::sync_queue(&item).await {
                Ok(_) => {
                    info!("Item {} processed successfully.", item.order_code);
                }
                Err(e) => {
                    error!("Error processing item {}: {:#}", item.order_code, e);
                    match repository::queue::update_error(&item.order_code, &e.to_string()).await {
                        Ok(_) => info!("Last error updated for order {}", item.order_code),
                        Err(err) => error!(
                            "Failed to update last error for order {}: {}",
                            item.order_code, err
                        ),
                    }
                }
            }

            if config::get().config.throttle > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    config::get().config.throttle as u64,
                ))
                .await;
            }
        }

        info!("Batch processing finished.");
    }
}
