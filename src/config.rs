use anyhow::Context;
use config::Config;
use serde::Deserialize;

pub fn load_config() -> anyhow::Result<QueueBotConfig> {
    let config = Config::builder()
        .add_source(config::File::with_name("queuebot"))
        .add_source(config::Environment::with_prefix("QUEUEBOT"))
        .build()?;

    dbg!(config.try_deserialize().context("could not parse config"))
}

#[derive(Deserialize, Debug)]
pub struct QueueBotConfig {
    pub mysql: MySqlConfig,
}

#[derive(Deserialize, Debug)]
pub struct MySqlConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}
