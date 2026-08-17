use anyhow::{Result, anyhow, bail};
use tracing::error;
use sibyl::{Environment, SessionPool};
use std::env;

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub user: String,
    pub password: String,
    pub db: String,
}

pub fn get_oci_env() -> Result<&'static Environment> {
    let oracle = sibyl::env().map_err(|e| anyhow!("Failed to initialize OCI environment: {e}"))?;
    Ok(Box::leak(Box::new(oracle)))
}

pub fn get_conn() -> Result<ConnectionInfo> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        error!("Forneça a string de conexão USUARIO/SENHA@HOST:PORT/SERVICO");
        bail!("Forneça a string de conexão USUARIO/SENHA@HOST:PORT/SERVICO");
    }
    let conn_str = &args[1];

    let parts: Vec<&str> = conn_str.splitn(2, '@').collect();
    if parts.len() != 2 {
        error!("Formato esperado: USUARIO/SENHA@HOST:PORT/SERVICO");
        bail!("Formato esperado: USUARIO/SENHA@HOST:PORT/SERVICO");
    }

    let creds = parts[0];
    let db = parts[1];

    let user_pass: Vec<&str> = creds.splitn(2, '/').collect();
    if user_pass.len() != 2 {
        error!("Formato de credenciais esperado: USUARIO/SENHA");
        bail!("Formato de credenciais esperado: USUARIO/SENHA");
    }

    if !db.contains(':') || !db.contains('/') {
        error!("Formato de rede esperado: HOST:PORT/SERVICO");
        bail!("Formato de rede esperado: HOST:PORT/SERVICO");
    }

    Ok(ConnectionInfo {
        user: user_pass[0].to_string(),
        password: user_pass[1].to_string(),
        db: db.to_string(),
    })
}

pub async fn init_pool(
    env: &'static Environment,
    conn_info: &ConnectionInfo,
) -> Result<SessionPool<'static>> {
    let pool = env.create_session_pool(
        &conn_info.db,
        &conn_info.user,
        &conn_info.password,
        1,
        1,
        5,
    ).await?;

    Ok(pool)
}
