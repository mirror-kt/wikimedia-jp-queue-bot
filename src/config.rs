use anyhow::Context;
use config::Config;
use serde::Deserialize;

pub fn load_config() -> anyhow::Result<QueueBotConfig> {
    from_path("queuebot")
}

pub fn from_path(path: impl AsRef<str>) -> anyhow::Result<QueueBotConfig> {
    let config = Config::builder()
        .add_source(config::File::with_name(path.as_ref()))
        .add_source(config::Environment::with_prefix("QUEUEBOT"))
        .build()?;

    config.try_deserialize().context("could not parse config")
}

#[derive(Deserialize, Debug)]
pub struct QueueBotConfig {
    pub mysql: MySqlConfig,
}

#[derive(Deserialize, Debug)]
pub struct MySqlConfig {
    pub connection_url: String,
}
