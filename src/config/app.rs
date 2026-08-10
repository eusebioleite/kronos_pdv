use crate::config;

pub fn get_app_config() -> Result<config::Config, anyhow::Error> {
    let config_path = "kronos_pdv.toml";

    let config_content = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(e) => anyhow::bail!("Failed to read config file '{}': {}", config_path, e),
    };

    let app_config: config::Config = match toml::from_str(&config_content) {
        Ok(config) => config,
        Err(e) => anyhow::bail!("Failed to parse config file '{}': {}", config_path, e),
    };
    Ok(app_config)
}