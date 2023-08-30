use std::collections::HashMap;
use std::fmt::{Debug, Display};

use mwbot::{Bot, SaveOptions};
use tracing::{info, warn};
use ulid::Ulid;

use super::{CommandStatus, OperationStatus};
use crate::category::{replace_category_tag, replace_redirect_category_template};
use crate::config::QueueBotConfig;
use crate::db::{store_operation, OperationType};
use crate::generator::list_category_members;
use crate::is_emergency_stopped;

#[tracing::instrument(skip(bot))]
pub async fn remove_category(
    bot: &Bot,
    config: &QueueBotConfig,
    id: &Ulid,
    category: impl AsRef<str> + Debug + Display,
    discussion_link: impl AsRef<str> + Debug + Display,
) -> CommandStatus {
    let discussion_link = discussion_link.as_ref();
    let category = category.as_ref();

    let mut category_members = list_category_members(bot, category, true, true);

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

        replace_category_tag(&html, category, &[]);
        replace_redirect_category_template(&html, category, &[]);

        let (_, res) = {
            let result = page
                .save(
                    html,
                    &SaveOptions::summary(&format!(
                        "BOT: [[:{}]]の削除 ([[{}|議論場所]]) (ID: {})",
                        category, discussion_link, id
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
                info!(page = &page_title, "Done");
                statuses.insert(page_title.clone(), OperationStatus::Remove);
            }

            result.unwrap() // SAFETY: Err(_) is covered
        };

        if let Err(err) = store_operation(
            &config.mysql,
            id,
            OperationType::Remove,
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
