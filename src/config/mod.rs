use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tracing::{error, info};

mod app;
mod company; 
mod requester;

// =====================================================================
// ITEMS
// =====================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Column {
    pub code: u32,
    pub name: String,
}

impl Column {
    pub fn validate(&self, path: &str) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err(format!("O 'name' na coluna [{}] não pode estar vazio.", path));
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
            return Err(format!("O 'name' no usuário [{}] não pode estar vazio.", path));
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
            return Err(format!("O 'name' no vendedor/requester [{}] não pode estar vazio.", path));
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
            return Err(format!("O 'code' da filial [company.{}] não pode ser 0.", location_name));
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
pub struct Config {
    pub auth_url: String,
    pub throttle: u16,
    pub interval: u16,
    pub api_url: String,
    pub crm_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub context_guid: String,
    pub port: u16,
    pub debug: bool,
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.auth_url.trim().is_empty() { return Err("O campo '[config].auth_url' não pode estar vazio.".into()); }
        if self.api_url.trim().is_empty() { return Err("O campo '[config].api_url' não pode estar vazio.".into()); }
        if self.crm_url.trim().is_empty() { return Err("O campo '[config].crm_url' não pode estar vazio.".into()); }
        if self.client_id.trim().is_empty() { return Err("O campo '[config].client_id' não pode estar vazio.".into()); }
        if self.client_secret.trim().is_empty() { return Err("O campo '[config].client_secret' não pode estar vazio.".into()); }
        if self.context_guid.trim().is_empty() { return Err("O campo '[config].context_guid' não pode estar vazio.".into()); }
        if self.port == 0 { return Err("A porta '[config].port' deve ser maior que 0.".into()); }
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
}

// =====================================================================
// FUNÇÃO DE INICIALIZAÇÃO
// =====================================================================

pub fn init() -> Result<RootConfig, anyhow::Error> {
    let path = std::path::Path::new("kronos_pdv.toml");

    let default_root = RootConfig {
        config: Config {
            auth_url: String::from("https://auth.dolphinsistemas.com.br/realms/dealercrm/protocol/openid-connect/token"),
            api_url: String::from("https://api-treino-kronospet.dealercrm.com.br"),
            crm_url: String::from("https://treino-kronospet.dealercrm.com.br"),
            client_id: String::from("treino-kronospet-1778695296950"),
            client_secret: String::from("K74VIJ6QgmqIY5IaSJ5ThjYvn2yvxbl0sinbxNyQeRX"),
            context_guid: String::from("af1a5eb4-6d54-11f0-8ce7-baccb1227f9a"),
            port: 9558,
            throttle: 1,
            interval: 5,
            debug: true,
        },
        company: HashMap::new(), 
        requesters: HashMap::new(),
    };

    if path.is_file() {
        info!("Arquivo de configuração encontrado em {}", path.display());
    } else {
        info!("Arquivo de configuração não encontrado. Criando um novo.");
        let toml_string = toml::to_string_pretty(&default_root)?;
        fs::write(path, toml_string)?;
        info!("Configuração padrão salva em {}", path.display());
    }

    let config_file = fs::read_to_string(path).map_err(|e| {
        error!("Erro ao ler arquivo de configuração: {}", e);
        anyhow::bail!("Erro ao ler arquivo de configuração: {}", e)
    })?;

    let root_config: RootConfig = toml::from_str(&config_file).map_err(|e| {
        error!("Erro de sintaxe ou campo obrigatório ausente no TOML: {}", e);
        anyhow::bail!("Erro de sintaxe ou campo obrigatório ausente no TOML: {}", e)
    })?;

    if let Err(err_msg) = root_config.validate() {
        error!("Configuração inválida no arquivo TOML: {}", err_msg);
        return anyhow::bail!("Configuração inválida no arquivo TOML: {}", err_msg);
    }

    Ok(root_config)
}