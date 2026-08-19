use anyhow::{Context, Result};
use sibyl::SessionPool;
use std::sync::OnceLock;

static ORACLE_POOL: OnceLock<SessionPool<'static>> = OnceLock::new();

pub async fn init_pool() -> Result<()> {
    let env = match sibyl::env() {
        Ok(env) => Box::leak(Box::new(env)),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to initialize Oracle OCI environment: {}",
                e
            ));
        }
    };

    let cfg = &crate::config::get().config.oracle;
    let db = format!("{}:{}/{}", cfg.host, cfg.port, cfg.database);

    let pool = env
        .create_session_pool(&db, &cfg.user, &cfg.password, 1, 1, 5)
        .await
        .with_context(|| {
            format!(
                "Failed to create Oracle session pool for user '{}' on '{}:{}/{}'",
                cfg.user, cfg.host, cfg.port, cfg.database
            )
        })?;

    ORACLE_POOL
        .set(pool)
        .map_err(|_| anyhow::anyhow!("database::init_pool() was called more than once"))?;

    Ok(())
}

pub fn get_pool() -> &'static SessionPool<'static> {
    ORACLE_POOL
        .get()
        .expect("database::init_pool() must be called before get_pool()")
}
