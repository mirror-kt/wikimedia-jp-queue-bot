use std::fmt::Debug;

use indexmap::IndexMap;
use mwbot::{Bot, Page, SaveOptions};
use tracing::warn;
use ulid::Ulid;

use super::{CommandStatus, OperationResult, OperationStatus};
use crate::db::{store_operation, OperationType};
use crate::generator::list_category_members;
use crate::is_emergency_stopped;
use crate::replacer::{get_category_replacers, CategoryReplacerList};

#[tracing::instrument(skip(bot))]
#[allow(clippy::too_many_arguments)]
pub async fn reassignment(
    bot: &Bot,
    id: &Ulid,
    from: impl AsRef<str> + Debug,
    to: impl AsRef<[String]> + Debug,
    discussion_link: impl AsRef<str> + Debug,
    include_article: bool,
    include_category: bool,
) -> CommandStatus {
    let from = from.as_ref();
    let to = to.as_ref();
    let discussion_link = discussion_link.as_ref();

    let replacers = get_category_replacers(bot, from, to);

    let mut category_members =
        list_category_members(bot, from, include_article, include_category).await;

    let mut statuses = IndexMap::new();
    while let Some(page) = category_members.recv().await {
        if is_emergency_stopped(bot).await {
            return CommandStatus::EmergencyStopped;
        }

        let Ok(page) = page else {
            warn!("Error while getting: {:?}", page);
            continue;
        };
        statuses.insert(
            page.title().to_string(),
            process_page(id, page, from, to, &replacers, discussion_link).await,
        );
    }

    if statuses.is_empty() {
        CommandStatus::Skipped
    } else {
        CommandStatus::Done { id: *id, statuses }
    }
}

#[tracing::instrument(skip(page), fields(title = page.title()))]
async fn process_page(
    command_id: &Ulid,
    page: Page,
    from: &str,
    to: &[String],
    replacers: &(impl CategoryReplacerList + Debug),
    discussion_link: &str,
) -> OperationResult {
    let html = page.html().await.map_err(|err| {
        warn!(message = "ページの取得中にエラーが発生しました", err = ?err);
        "ページの取得中にエラーが発生しました".to_string()
    })?;

    let (replaced, is_changed) = replacers.replace_all(html).await.map_err(|err| {
        warn!(message = "カテゴリの変更中にエラーが発生しました", err = ?err);
        "カテゴリの変更中にエラーが発生しました".to_string()
    })?;

    if !is_changed {
        return Ok(OperationStatus::Skipped);
    }

    let (_, res) = page
        .save(
            replaced,
            &SaveOptions::summary(&format!(
                "BOT: [[:{}]]から{}へ変更 ([[{}|議論場所]]) (ID: {})",
                &from,
                &to.iter()
                    .map(|cat| format!("[[:{}]]", cat))
                    .collect::<Vec<_>>()
                    .join(","),
                &discussion_link,
                command_id,
            )),
        )
        .await
        .map_err(|err| {
            warn!(message = "ページの保存に失敗しました", err = ?err);
            "ページの保存に失敗しました".to_string()
        })?;

    if let Some(new_revid) = res.newrevid {
        store_operation(
            command_id,
            OperationType::Reassignment,
            res.pageid,
            new_revid,
        )
        .await
        .map_err(|err| {
            warn!(message = "データベースへのオペレーション保存に失敗しました", err = ?err);
            "データベースへのオペレーション保存に失敗しました".to_string()
        })?;
    }

    Ok(OperationStatus::Reassignment)
}
