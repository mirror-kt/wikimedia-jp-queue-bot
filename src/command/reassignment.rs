use std::collections::HashMap;
use std::fmt::{Debug, Display};

use indexmap19::indexmap;
use mwbot::parsoid::prelude::*;
use mwbot::{Bot, SaveOptions};
use tracing::{info, warn};
use ulid::Ulid;

use super::{CommandStatus, OperationStatus};
use crate::action::get_page_info;
use crate::category::{replace_category_tag, replace_redirect_category_template};
use crate::config::QueueBotConfig;
use crate::db::{store_operation, OperationType};
use crate::generator::list_category_members;
use crate::is_emergency_stopped;

#[tracing::instrument(skip(bot, config))]
#[allow(clippy::too_many_arguments)]
pub async fn reassignment<'to>(
    bot: &Bot,
    config: &QueueBotConfig,
    id: &Ulid,
    from: impl AsRef<str> + Debug + Display,
    to: impl AsRef<[String]> + Debug,
    discussion_link: impl AsRef<str> + Debug,
    include_article: bool,
    include_category: bool,
) -> CommandStatus {
    let from = from.as_ref();
    let to = to.as_ref();
    let discussion_link = discussion_link.as_ref();

    let mut category_members =
        list_category_members(bot, from, include_article, include_category).await;

    let mut statuses = HashMap::new();
    while let Some(page) = category_members.recv().await {
        if is_emergency_stopped(bot).await {
            return CommandStatus::EmergencyStopped;
        }

        let Ok(page) = page else {
            warn!("Error while getting: {:?}", page);
            continue;
        };
        if page.is_category() && !include_category {
            continue;
        }
        if !page.is_category() && !include_article {
            continue;
        }
        let page_title = page.title().to_string();

        let Ok(page_info) = get_page_info(bot, page.title()).await else {
            warn!(
                page = &page_title,
                "{}", "ページのメタデータの取得に失敗しました"
            );
            statuses.insert(
                page_title,
                OperationStatus::Error("ページのメタデータの取得に失敗しました".to_string()),
            );
            continue;
        };
        // 対象ページが全保護されている場合はスキップ
        if page_info
            .protection
            .iter()
            .any(|protection| protection.type_ == *"edit" && protection.level == *"sysop")
        {
            warn!(
                page = &page_title,
                "ページが全保護されているため編集できませんでした"
            );
            statuses.insert(
                page_title,
                OperationStatus::Error("全保護されているため編集できませんでした".to_string()),
            );
            continue;
        };

        let Ok(html) = page.html().await.map(|html| html.into_mutable()) else {
            warn!(page = &page_title, "Error while getting html");
            continue;
        };

        replace_category_tag(&html, from, to);
        replace_redirect_category_template(&html, from, to);

        let (_, res) = {
            let result = page
                .save(
                    html,
                    &SaveOptions::summary(&format!(
                        "BOT: [[:{}]]から{}へ変更 ([[{}|議論場所]]) (ID: {})",
                        &from,
                        &to.iter()
                            .map(|cat| format!("[[:{}]]", cat))
                            .collect::<Vec<_>>()
                            .join(","),
                        &discussion_link,
                        id,
                    )),
                )
                .await;
            if let Err(err) = result {
                warn!(page = &page_title, "ページの保存に失敗しました: {}", err);
                statuses.insert(
                    page_title,
                    OperationStatus::Error("ページの保存に失敗しました".to_string()),
                );
                continue;
            } else {
                statuses.insert(page_title.clone(), OperationStatus::Reassignment);
                info!(page = &page_title, "Done");
            }

            result.unwrap() // SAFETY: Err(_) is covered
        };

        if let Err(err) = store_operation(
            &config.mysql,
            id,
            OperationType::Reassignment,
            res.pageid,
            res.newrevid,
        )
        .await
        {
            warn!("{}", err);
            statuses.insert(
                page_title,
                OperationStatus::Error(
                    "データベースへのオペレーション保存に失敗しました".to_string(),
                ),
            );
            continue;
        };
    }

    if statuses.is_empty() {
        CommandStatus::Skipped
    } else {
        try_add_speedy_deletion_template(bot, id, from, discussion_link, statuses).await
    }
}

async fn try_add_speedy_deletion_template(
    bot: &Bot,
    id: &Ulid,
    from: &str,
    discussion_link: &str,
    statuses: HashMap<String, OperationStatus>,
) -> CommandStatus {
    let mut category_members = list_category_members(bot, from, true, true).await;
    if category_members.recv().await.is_none() {
        let Ok(from_page) = bot.page(from) else {
            warn!("Error while getting page: {:?}", from);
            return CommandStatus::Error {
                id: *id,
                statuses,
                message: "即時削除テンプレートの貼り付けの際、カテゴリページの取得に失敗しました"
                    .to_string(),
            };
        };
        let content = Wikicode::new("");
        let template = &Template::new(
            "即時削除",
            &indexmap! {
                "1".to_string() => "カテゴリ6".to_string(),
                "2".to_string() => format!("[[{}]]", discussion_link),
            },
        )
        .expect("unhappened");
        content.append(template);
        if let Err(err) = from_page
            .save(content, &SaveOptions::summary("BOT: 即時削除 (カテゴリ6)"))
            .await
        {
            warn!("Error while saving page: {:?}", err);
            CommandStatus::Error {
                id: *id,
                statuses,
                message: "カテゴリへの即時削除テンプレートの貼り付けに失敗しました".to_string(),
            }
        } else {
            CommandStatus::Done { id: *id, statuses }
        }
    } else {
        CommandStatus::Done { id: *id, statuses }
    }
}
