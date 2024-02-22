use std::hash::{Hash, Hasher};

use anyhow::Context;
use config::Config;
use indexmap::IndexMap;
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

#[derive(Deserialize, Debug)]
pub struct OnWikiConfig {
    pub discussion_summary_icon_bindings: Vec<DiscussionSummaryIconBindings>,
}

#[derive(Deserialize, Debug)]
pub struct DiscussionSummaryIconBindings {
    pub main: Template,
    pub alternatives: Vec<Template>,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub params: IndexMap<String, Option<String>>,
}

impl Hash for Template {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        // workaround for state.write_length_prefix(self.params.len());
        state.write_usize(self.params.len());
        for item in &self.params {
            item.hash(state);
        }
    }
}

impl Template {
    pub fn matches(&self, other: &mwbot::parsoid::Template) -> bool {
        if self.name != other.name() {
            return false;
        }
        self.params.iter().all(|(k, v)| other.param(k) == *v)
    }
}

impl TryFrom<&Template> for mwbot::parsoid::Template {
    type Error = mwbot::parsoid::Error;

    fn try_from(value: &Template) -> Result<Self, Self::Error> {
        let params = value
            .params
            .iter()
            .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
            .collect();
        Self::new(&value.name, &params)
    }
}
