use anyhow::Context as _;
use sqlx::{Connection as _, Executor as _, MySqlConnection};
use sqlx::mysql::MySqlConnectOptions;
use tokio_retry::Retry;
use tokio_retry::strategy::{ExponentialBackoff, jitter};
use ulid::Ulid;
use uuid::Uuid;

use crate::config::MySqlConfig;

pub async fn get_connection(config: &MySqlConfig) -> anyhow::Result<MySqlConnection> {
    MySqlConnection::connect_with(
        &MySqlConnectOptions::new()
            .host(&config.host)
            .port(config.port)
            .username(&config.user)
            .password(&config.password)
            .database(&config.database),
    )
        .await
        .context("could not get connection")
}

pub async fn store_command(
    config: &MySqlConfig,
    command_id: &Ulid,
    command_type: CommandType,
    params: serde_json::Value,
) -> anyhow::Result<()> {
    let command_id: Uuid = (*command_id).into();
    let retry_strategy = ExponentialBackoff::from_millis(5).map(jitter).take(3);

    Retry::spawn(retry_strategy, || async {
        let mut conn = get_connection(config)
            .await
            .context("could not get connection")?;

        let query = sqlx::query!(
            "INSERT INTO commands VALUES (?, ?, ?)",
            command_id.as_bytes().as_slice(),
            command_type,
            params
        );

        conn.execute(query)
            .await
            .context("could not execute query")?;

        Ok(())
    })
        .await
}

#[derive(sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
pub enum CommandType {
    ReassignmentAll,
    ReassignmentArticle,
    ReassignmentCategory,
    RemoveCategory,
    DuplicateCategory,
}

pub async fn store_operation(
    config: &MySqlConfig,
    command_id: &Ulid,
    operation_type: OperationType,
    page_id: u32,
    new_revid: Option<u64>,
) -> anyhow::Result<()> {
    let id: Uuid = Ulid::new().into();
    let command_id: Uuid = (*command_id).into();

    let retry_strategy = ExponentialBackoff::from_millis(5).map(jitter).take(3);

    Retry::spawn(retry_strategy, || async {
        let mut conn = get_connection(config)
            .await
            .context("could not get connection")?;
        let query = sqlx::query!(
            "INSERT INTO operations VALUES (?, ?, ?, ?, ?)",
            id.as_bytes().as_slice(),
            command_id.as_bytes().as_slice(),
            operation_type,
            page_id,
            new_revid
        );

        conn.execute(query)
            .await
            .context("could not execute query")?;

        Ok(())
    })
        .await
}

#[derive(sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
pub enum OperationType {
    Reassignment,
    Remove,
    Duplicate,
}
