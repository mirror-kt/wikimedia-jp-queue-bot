use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Debug;

use mwbot::{Bot, SaveOptions};
use tracing::{warn, info};
use ulid::Ulid;

use super::CommandStatus;
use crate::category::{replace_category_tag, replace_redirect_category_template};
use crate::command::OperationStatus;
use crate::config::QueueBotConfig;
use crate::db::{store_operation, OperationType};
use crate::generator::list_category_members;
use crate::is_emergency_stopped;

#[tracing::instrument(skip(bot, config))]
pub async fn duplicate_category<'source, 'dest>(
    bot: &Bot,
    config: &QueueBotConfig,
    id: &Ulid,
    source: impl Into<Cow<'source, str>> + Debug,
    dest: impl Into<Cow<'dest, str>> + Debug,
    discussion_link: impl AsRef<str> + Debug,
) -> CommandStatus {
    let source: String = source.into().into_owned();
    let dest: String = dest.into().into_owned();
    let to = &[source.clone(), dest.clone()];
    let discussion_link = discussion_link.as_ref();

    let mut category_members = list_category_members(bot, &source, true, true);

    let mut statuses = HashMap::new();
    while let Some(page) = category_members.recv().await {
        if is_emergency_stopped(bot).await {
            return CommandStatus::EmergencyStopped;
        }

        let Ok(page) = page else {
            warn!("Error while searching: {:?}", page);
            continue;
        };
        let Ok(html) = page.html().await.map(|html| html.into_mutable()) else {
            warn!("Error while getting html: {:?}", page);
            continue;
        };
        let page_title = page.title().to_string();

        replace_category_tag(&html, &source, to);
        replace_redirect_category_template(&html, &source, to);

        let (_, res) = {
            let result = page
                .save(
                    html,
                    &SaveOptions::summary(&format!(
                        "BOT: [[:{}]]を [[:{}]]に複製 ([[{}|議論場所]]) (ID: {})",
                        &source, &dest, discussion_link, id
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
                statuses.insert(page_title.clone(), OperationStatus::Duplicate);
                info!(page = &page_title, "Done");
            }

            result.unwrap() // SAFETY: Err(_) is covered
        };

        if let Err(err) = store_operation(
            &config.mysql,
            id,
            OperationType::Duplicate,
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
    CommandStatus::Done { id: *id, statuses }
}
