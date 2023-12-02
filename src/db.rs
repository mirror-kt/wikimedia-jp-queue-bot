use anyhow::Context as _;
use backon::{ExponentialBuilder, Retryable as _};
use sqlx::mysql::MySqlQueryResult;
use sqlx::{Execute, Executor as _, MySql, MySqlPool};
use tokio::sync::OnceCell;
use ulid::Ulid;
use uuid::Uuid;

use crate::config::MySqlConfig;

static POOL: OnceCell<MySqlPool> = OnceCell::const_new();

pub async fn init(config: &MySqlConfig) -> anyhow::Result<()> {
    POOL.set(MySqlPool::connect(&config.connection_url).await?)?;
    Ok(())
}

async fn execute_query<'q, E: 'q>(query: impl Fn() -> E) -> anyhow::Result<MySqlQueryResult>
where
    E: Execute<'q, MySql>,
{
    let query = query();

    POOL.get()
        .unwrap()
        .execute(query)
        .await
        .context("could not execute query")
}

pub async fn store_command(
    command_id: &Ulid,
    command_type: CommandType,
    params: serde_json::Value,
) -> anyhow::Result<()> {
    let command_id: Uuid = (*command_id).into();
    let save = || async {
        execute_query(|| {
            sqlx::query!(
                "INSERT INTO commands VALUES (?, ?, ?)",
                command_id.as_bytes().as_slice(),
                command_type,
                params
            )
        })
        .await
        .context("could not execute query")?;
        Ok::<_, anyhow::Error>(())
    };

    save.retry(
        &ExponentialBuilder::default()
            .with_jitter()
            .with_max_times(5),
    )
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
    command_id: &Ulid,
    operation_type: OperationType,
    page_id: u32,
    new_revid: u64,
) -> anyhow::Result<()> {
    let id: Uuid = Ulid::new().into();
    let command_id: Uuid = (*command_id).into();
    let save = || async {
        execute_query(|| {
            sqlx::query!(
                "INSERT INTO operations VALUES (?, ?, ?, ?, ?)",
                id.as_bytes().as_slice(),
                command_id.as_bytes().as_slice(),
                operation_type,
                page_id,
                new_revid
            )
        })
        .await
        .context("could not execute query")?;
        Ok::<_, anyhow::Error>(())
    };

    save.retry(
        &ExponentialBuilder::default()
            .with_jitter()
            .with_max_times(5),
    )
    .await
}

#[derive(sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
pub enum OperationType {
    Reassignment,
    Remove,
    Duplicate,
}
