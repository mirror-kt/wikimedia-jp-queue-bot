use std::fmt::Debug;

use indexmap::IndexMap;
use mwbot::{Bot, Page, SaveOptions};
use tracing::warn;
use ulid::Ulid;

use super::{CommandStatus, OperationResult};
use crate::command::OperationStatus;
use crate::db::{store_operation, OperationType};
use crate::generator::list_category_members;
use crate::is_emergency_stopped;
use crate::replacer::{get_category_replacers, CategoryReplacerList};

#[tracing::instrument(skip(bot))]
pub async fn duplicate_category(
    bot: &Bot,
    id: &Ulid,
    source: impl Into<String> + Debug,
    dest: impl Into<String> + Debug,
    discussion_link: impl AsRef<str> + Debug,
) -> CommandStatus {
    let (source, dest) = (source.into(), dest.into());
    let to = &[source.clone(), dest.clone()];
    let discussion_link = discussion_link.as_ref();

    let replacers = get_category_replacers(bot, &source, to);

    let mut category_members = list_category_members(bot, &source, true, true).await;

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
            process_page(id, page, &source, to, &replacers, discussion_link).await,
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
    source: &str,
    dest: &[String],
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
                "BOT: [[:{}]]を{}に複製 ([[{}|議論場所]]) (ID: {})",
                &source,
                &dest
                    .iter()
                    .map(|d| format!("[[:{d}]]"))
                    .collect::<Vec<_>>()
                    .join(","),
                discussion_link,
                command_id,
            )),
        )
        .await
        .map_err(|err| {
            warn!(message = "ページの保存に失敗しました", err = ?err);
            "ページの保存に失敗しました".to_string()
        })?;

    if let Some(new_revid) = res.newrevid {
        store_operation(command_id, OperationType::Duplicate, res.pageid, new_revid)
            .await
            .map_err(|err| {
                warn!(message = "データベースへのオペレーション保存に失敗しました", err = ?err);
                "データベースへのオペレーション保存に失敗しました".to_string()
            })?;
    }

    Ok(OperationStatus::Duplicate)
}
