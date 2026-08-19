use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;
use tracing::{error, info};

// =====================================================================
// ITEMS
// =====================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Column {
    pub code: u32,
    pub name: String,
    pub responsible: String,
    #[serde(default)]
    pub product_override: HashMap<String, String>,
}

impl Column {
    pub fn validate(&self, path: &str) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err(format!("The 'name' in [column.{}] cannot be empty.", path));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub code: u32,
    pub name: String,
}

impl User {
    pub fn validate(&self, path: &str) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err(format!("The 'name' in [user.{}] cannot be empty.", path));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Requester {
    pub code: u32,
    pub name: String,
}

impl Requester {
    pub fn validate(&self, path: &str) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err(format!(
                "The 'name' in [requester.{}] cannot be empty.",
                path
            ));
        }
        Ok(())
    }
}

// =====================================================================
// GRUPOS
// =====================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Company {
    pub code: u32,
    #[serde(default)]
    pub columns: HashMap<String, Column>,
    #[serde(default)]
    pub users: HashMap<String, User>,
}

impl Company {
    pub fn validate(&self, location_name: &str) -> Result<(), String> {
        if self.code == 0 {
            return Err(format!(
                "The 'code' in [company.{}] cannot be 0.",
                location_name
            ));
        }

        for (column_key, column_info) in &self.columns {
            let path = format!("company.{}.columns.{}", location_name, column_key);
            column_info.validate(&path)?;
        }

        for (user_key, user_info) in &self.users {
            let path = format!("company.{}.users.{}", location_name, user_key);
            user_info.validate(&path)?;
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DbConfig {
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: String,
    pub database: String,
}

impl DbConfig {
    pub fn validate(&self, label: &str) -> Result<(), String> {
        for (field, value) in [
            ("user", &self.user),
            ("password", &self.password),
            ("host", &self.host),
            ("port", &self.port),
            ("database", &self.database),
        ] {
            if value.trim().is_empty() {
                return Err(format!(
                    "The field '[config.{}].{}' cannot be empty.",
                    label, field
                ));
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub auth_url: String,
    pub throttle: u16,
    pub interval: u16,
    pub api_url: String,
    pub crm_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub context_guid: String,
    pub mysql: DbConfig,
    pub oracle: DbConfig,
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.auth_url.trim().is_empty() {
            return Err("The field '[config].auth_url' cannot be empty.".into());
        }
        if self.api_url.trim().is_empty() {
            return Err("The field '[config].api_url' cannot be empty.".into());
        }
        if self.crm_url.trim().is_empty() {
            return Err("The field '[config].crm_url' cannot be empty.".into());
        }
        if self.client_id.trim().is_empty() {
            return Err("The field '[config].client_id' cannot be empty.".into());
        }
        if self.client_secret.trim().is_empty() {
            return Err("The field '[config].client_secret' cannot be empty.".into());
        }
        if self.context_guid.trim().is_empty() {
            return Err("The field '[config].context_guid' cannot be empty.".into());
        }
        self.mysql.validate("mysql")?;
        self.oracle.validate("oracle")?;
        Ok(())
    }
}

// =====================================================================
// STRUCT RAIZ DO ARQUIVO TOML
// =====================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RootConfig {
    pub config: Config,
    #[serde(default)]
    pub company: HashMap<String, Company>,
    #[serde(default)]
    pub requesters: HashMap<String, Requester>,
}

#[allow(dead_code)]
impl RootConfig {
    pub fn validate(&self) -> Result<(), String> {
        self.config.validate()?;

        for (location_name, location_data) in &self.company {
            location_data.validate(location_name)?;
        }

        for (req_key, req_info) in &self.requesters {
            let path = format!("requesters.{}", req_key);
            req_info.validate(&path)?;
        }

        Ok(())
    }

    pub fn default_requester_code(&self) -> u32 {
        self.requesters.get("default").map(|r| r.code).unwrap_or(0)
    }

    pub fn get_requester_by_name(&self, name: &str) -> Option<&Requester> {
        self.requesters
            .values()
            .find(|r| r.name.trim().eq_ignore_ascii_case(name))
    }

    pub fn get_column_by_company(&self, company_code: u32, column_name: &str) -> Option<&Column> {
        for company in self.company.values() {
            if company.code == company_code {
                return company
                    .columns
                    .values()
                    .find(|c| c.name.trim().eq_ignore_ascii_case(column_name));
            }
        }
        None
    }

    pub fn get_responsible(
        &self,
        company: &Company,
        column: &Column,
        commercial: u32,
        tipo_produto: &str,
    ) -> u32 {
        // 1. Verifica se existe uma regra específica para o tipo de produto
        let user_key = if let Some(override_user) = column.product_override.get(tipo_produto) {
            override_user
        } else {
            &column.responsible
        };

        if user_key == "comercial" {
            return commercial;
        }

        company
            .users
            .get(user_key)
            .map(|u| u.code)
            .unwrap_or(commercial)
    }
}

// =====================================================================
// GLOBAL SINGLETON
// =====================================================================

/// Process-wide config instance. Populated once by `init()` at startup.
static CONFIG: OnceLock<RootConfig> = OnceLock::new();

/// Returns a reference to the global config.
/// Panics if called before `init()`.
pub fn get() -> &'static RootConfig {
    CONFIG
        .get()
        .expect("config::init() must be called before config::get()")
}

// =====================================================================
// FUNÇÃO DE INICIALIZAÇÃO
// =====================================================================

pub fn init() -> anyhow::Result<()> {
    let path = std::path::Path::new("kronos_pdv.toml");

    if !path.is_file() {
        anyhow::bail!(
            "Config file not found in '{}'. Create the file with the credentials before starting.",
            path.display()
        );
    }

    info!("Config file found in {}", path.display());

    let config_file = fs::read_to_string(path)
        .with_context(|| format!("Error reading config file in {}", path.display()))?;

    let root_config: RootConfig = toml::from_str(&config_file).with_context(|| {
        format!(
            "Syntax error or missing required field in TOML ({})",
            path.display()
        )
    })?;

    if let Err(err_msg) = root_config.validate() {
        error!("Invalid config in TOML file: {}", err_msg);
        anyhow::bail!(
            "Invalid config in TOML file ({}): {}",
            path.display(),
            err_msg
        );
    }

    // Store in the global singleton. Fails only if called twice.
    CONFIG
        .set(root_config)
        .map_err(|_| anyhow::anyhow!("config::init() was called more than once"))?;

    Ok(())
}
