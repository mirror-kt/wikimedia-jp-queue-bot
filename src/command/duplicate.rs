use std::borrow::Cow;
use std::fmt::Debug;

use mwbot::{Bot, SaveOptions};
use tracing::warn;

use super::Status;
use crate::category::{replace_category_tag, replace_redirect_category_template};
use crate::generator::list_category_members;
use crate::is_emergency_stopped;

#[tracing::instrument(skip(bot))]
pub async fn duplicate_category<'source, 'dest>(
    bot: &Bot,
    source: impl Into<Cow<'source, str>> + Debug,
    dest: impl Into<Cow<'dest, str>> + Debug,
    discussion_link: impl AsRef<str> + Debug,
) -> anyhow::Result<Status> {
    let source: String = source.into().into_owned();
    let dest: String = dest.into().into_owned();
    let to = &[source.clone(), dest.clone()];
    let discussion_link = discussion_link.as_ref();

    let mut category_members = list_category_members(bot, &source, true, true);

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

        replace_category_tag(&html, &source, to);
        replace_redirect_category_template(&html, &source, to);

        let _ = page
            .save(
                html,
                &SaveOptions::summary(&format!(
                    "BOT: {} カテゴリを {} カテゴリに複製 ([[{}]])",
                    &source, &dest, discussion_link
                )),
            )
            .await;
        done_count += 1;
    }
    Ok(Status::Done { done_count })
}
