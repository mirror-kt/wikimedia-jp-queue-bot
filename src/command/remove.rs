use std::fmt::{Debug, Display};

use indexmap19::indexmap;
use mwbot::parsoid::prelude::*;
use mwbot::{Bot, SaveOptions};
use tracing::warn;

use super::Status;
use crate::category::{replace_category_tag, replace_redirect_category_template};
use crate::generator::list_category_members;
use crate::is_emergency_stopped;

#[tracing::instrument(skip(bot))]
pub async fn remove_category(
    bot: &Bot,
    category: impl AsRef<str> + Debug + Display,
    discussion_link: impl AsRef<str> + Debug + Display,
) -> anyhow::Result<Status> {
    let discussion_link = discussion_link.as_ref();
    let category = category.as_ref();

    let mut category_members = list_category_members(bot, category, true, true);

    let mut done_count = 0;
    while let Some(page) = category_members.recv().await {
        if is_emergency_stopped(bot).await {
            return Ok(Status::EmergencyStopped);
        }

        let Ok(page) = page else {
            warn!("Error while searching: {:?}", page);
            continue;
        };

        let Ok(html) = page.html().await.map(|html| html.into_mutable()) else {
            warn!("Error while getting html: {:?}", page);
            continue;
        };

        replace_category_tag(&html, category, &[]);
        replace_redirect_category_template(&html, category, &[]);

        let _ = page
            .save(
                html,
                &SaveOptions::summary(&format!(
                    "BOT: カテゴリ [[:{}]]の削除 ([[{}|議論場所]])",
                    category, discussion_link
                )),
            )
            .await;
        done_count += 1;
    }

    Ok(Status::Done { done_count })
}
