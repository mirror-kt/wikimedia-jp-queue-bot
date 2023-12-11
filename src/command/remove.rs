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
pub async fn remove_category(
    bot: &Bot,
    id: &Ulid,
    category: impl AsRef<str> + Debug,
    discussion_link: impl AsRef<str> + Debug,
) -> CommandStatus {
    let category = category.as_ref();
    let discussion_link = discussion_link.as_ref();

    let replacers = get_category_replacers(bot, category, &[]);

    let mut category_members = list_category_members(bot, category, true, true).await;

    let mut statuses = IndexMap::new();
    while let Some(page) = category_members.recv().await {
        if is_emergency_stopped(bot).await {
            return CommandStatus::EmergencyStopped;
        }

        let Ok(page) = page else {
            warn!("Error while searching: {:?}", page);
            continue;
        };
        statuses.insert(
            page.title().to_string(),
            process_page(id, page, category, &replacers, discussion_link).await,
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
    category: &str,
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
                "BOT: [[:{}]]の削除 ([[{}|議論場所]]) (ID: {})",
                category, discussion_link, command_id
            )),
        )
        .await
        .map_err(|err| {
            warn!(message = "ページの保存に失敗しました", err = ?err);
            "ページの保存に失敗しました".to_string()
        })?;

    if let Some(new_revid) = res.newrevid {
        store_operation(command_id, OperationType::Remove, res.pageid, new_revid)
            .await
            .map_err(|err| {
                warn!(message = "データベースへのオペレーション保存に失敗しました", err = ?err);
                "データベースへのオペレーション保存に失敗しました".to_string()
            })?;
    }

    Ok(OperationStatus::Remove)
}
