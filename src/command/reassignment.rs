use std::fmt::{Debug, Display};

use if_chain::if_chain;
use mwbot::{Bot, SaveOptions};
use tracing::warn;

use super::Status;
use crate::action::move_page;
use crate::category::{replace_category_tag, replace_redirect_category_template};
use crate::generator::list_category_members;
use crate::is_emergency_stopped;

#[tracing::instrument(skip(bot))]
pub async fn reassignment<'to>(
    bot: &Bot,
    from: impl AsRef<str> + Debug + Display,
    to: impl AsRef<[String]> + Debug,
    discussion_link: impl AsRef<str> + Debug,
    include_article: bool,
    include_category: bool,
) -> anyhow::Result<Status> {
    let from = from.as_ref();
    let to = to.as_ref();
    let discussion_link = discussion_link.as_ref();

    let to_page = bot.page(&to[0])?;
    if !to_page.exists().await? {
        move_page(bot, &from, &to[0], format!("BOT: {}", &discussion_link)).await?;
    }
    if_chain! {
        if let Ok(from_page) = bot.page(from);
        if let Ok(html) = from_page.html().await;
        if let Some(revision_id) = html.revision_id();

        then {
            for to_page in to[1..].iter().map(|to| bot.page(to)).filter_map(|to_page| to_page.ok()) {
                if to_page.exists().await.unwrap_or(true) {
                    continue
                }
                let _ = to_page.save(
                    html.clone(),
                    &SaveOptions::summary(&format!(
                        "BOT: https://ja.wikipedia.org/w/index.php?title={}&oldid={} から複製 ({})",
                        from_page.title(),
                        revision_id,
                        discussion_link,
                    ))
                ).await;
            }
        }
    }

    let mut category_members = list_category_members(bot, from, include_article, include_category);

    let mut done_count = 0;
    while let Some(page) = category_members.recv().await {
        if is_emergency_stopped(bot).await {
            return Ok(Status::EmergencyStopped);
        }

        let Ok(page) = page else {
            warn!("Error while searching: {:?}", page);
            continue;
        };
        if page.is_category() && !include_category {
            continue;
        }
        if !page.is_category() && !include_article {
            continue;
        }

        let Ok(html) = page.html().await.map(|html| html.into_mutable()) else {
            warn!("Error while getting html: {:?}", page);
            continue;
        };

        replace_category_tag(&html, from, to);
        replace_redirect_category_template(&html, from, to);

        let _ = page
            .save(
                html,
                &SaveOptions::summary(&format!("BOT: カテゴリの変更 ([[{}]])", &discussion_link)),
            )
            .await;
        done_count += 1;
    }

    // // カテゴリに所属するページがなくなった場合、即時削除を要請する
    // // TODO: 即時削除の方針の改定までコメントアウト
    // let mut category_members = CategoryMembers::new(from.to_string()).generate(bot);
    // if category_members.recv().await.is_none() {
    //     let Ok(from_page) = bot.page(from) else {
    //         warn!("Error while getting page: {:?}", from);
    //         return Ok(Status::Done { done_count });
    //     };
    //     if let Err(err) = from_page
    //         .save(
    //             format!("{{即時削除|カテゴリ6|{}}}", discussion_link),
    //             &SaveOptions::summary(&format!("BOT: 即時削除 ({})", discussion_link)),
    //         )
    //         .await
    //     {
    //         warn!("Error while saving page: {:?}", err);
    //     };
    // }
    Ok(Status::Done { done_count })
}
