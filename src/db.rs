use anyhow::Context as _;
use backon::{ExponentialBuilder, Retryable as _};
use sqlx::{query, Executor as _, MySqlPool, QueryBuilder};
use tap::Tap;
use tokio::sync::OnceCell;
use ulid::Ulid;
use uuid::Uuid;

use crate::command::Command;
use crate::config::MySqlConfig;

static POOL: OnceCell<MySqlPool> = OnceCell::const_new();

pub async fn init(config: &MySqlConfig) -> anyhow::Result<()> {
    POOL.set(MySqlPool::connect(&config.connection_url).await?)?;
    Ok(())
}

fn pool() -> &'static MySqlPool {
    POOL.get().expect("Database Pool is not initialized")
}

pub async fn store_command<R>(command: &Command<R>) -> anyhow::Result<()> {
    let command_id: Uuid = command.id.into();
    let command_id = command_id.as_bytes().as_slice();

    let save = || async {
        let pool = pool();
        let mut tx = pool.begin().await.context("could not begin transaction")?;

        query!(
            "INSERT INTO commands (id, command_type, discussion_link) VALUES (?, ?, ?)",
            &command_id,
            &command.command_type,
            &command.discussion_link
        )
        .execute(&mut *tx)
        .await?;

        let mut insert_namespaces_query =
            QueryBuilder::new("INSERT INTO command_target_namespaces (command_id, namespace) ")
                .tap_mut(|builder| {
                    builder.push_values(&command.namespaces, |mut b, ns| {
                        b.push_bind(command_id).push_bind(ns);
                    });
                });
        insert_namespaces_query.build().execute(&mut *tx).await?;

        query!(
            "INSERT INTO command_from_categories (command_id, category) VALUES (?, ?)",
            &command_id,
            &command.from,
        )
        .execute(&mut *tx)
        .await?;

        let mut insert_to_categories_query = QueryBuilder::new(
            "INSERT INTO command_to_categories (command_id, category) ",
        )
        .tap_mut(|builder| {
            builder.push_values(&command.to, |mut b, to| {
                b.push_bind(command_id).push_bind(to);
            });
        });
        insert_to_categories_query.build().execute(&mut *tx).await?;

        tx.commit().await?;

        Ok(())
    };

    save.retry(
        &ExponentialBuilder::default()
            .with_jitter()
            .with_max_times(5),
    )
    .await
}

#[derive(sqlx::Type, Debug)]
#[sqlx(rename_all = "lowercase")]
pub enum CommandType {
    Reassignment,
    Remove,
    Duplicate,
}

pub async fn store_operation(
    command_id: &Ulid,
    page_id: u32,
    new_revid: u64,
) -> anyhow::Result<()> {
    let id: Uuid = Ulid::new().into();
    let command_id: Uuid = (*command_id).into();
    let save = || async {
        let pool = pool();
        sqlx::query!(
            "INSERT INTO operations VALUES (?, ?, ?, ?)",
            id.as_bytes().as_slice(),
            command_id.as_bytes().as_slice(),
            page_id,
            new_revid
        )
        .execute(pool)
        .await?;

        Ok(())
    };

    save.retry(
        &ExponentialBuilder::default()
            .with_jitter()
            .with_max_times(5),
    )
    .await
}
