use std::env;

use anyhow::Context;
use futures_util::{Stream, StreamExt};
use mwbot::{Bot, SaveOptions};
use sqlx::{query, Execute, Executor, FromRow, MySql, MySqlPool, QueryBuilder};
use ulid::Ulid;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let command_ids = env::args()
        .skip(1)
        .map(|arg| Ulid::from_string(&arg).context("could not parse ULID"))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| <Ulid as Into<Uuid>>::into(id))
        .collect::<Vec<_>>();
    let bot = Bot::from_default_config().await?;
    let save_opt = SaveOptions::summary("BOT: Undo operation");

    let config = queuebot::config::from_path("queuebot.local")?;
    let pool = MySqlPool::connect(&config.mysql.connection_url).await?;

    let mut query: QueryBuilder<'_, MySql> =
        QueryBuilder::new("SELECT page_id, rev_id FROM operations WHERE command_id IN (");
    let mut separated = query.separated(", ");
    command_ids.iter().for_each(|id| {
        separated.push_bind(id);
    });
    separated.push_unseparated(")");

    let query = query.build_query_as::<Operation>();
    let mut operations = query.fetch(&pool);

    while let Some(operation) = operations.next().await {
        let operation = match operation {
            Ok(operation) => operation,
            Err(err) => {
                tracing::error!(err = ?err);
                continue;
            }
        };
        let page = bot.page_from_id(operation.page_id as u64).await?;
        let page_title = page.title();
        if let Err(err) = page.undo(operation.rev_id as u64, None, &save_opt).await {
            tracing::error!(title = page_title, err = ?err);
        }
    }
    Ok(())
}

#[derive(Debug, FromRow)]
struct Operation {
    page_id: i32,
    rev_id: i64,
}
