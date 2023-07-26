use mwbot::generators::{Generator as _, Search};
use mwbot::{Bot, SaveOptions};
use tracing::warn;

use crate::action::move_page;
use crate::category::{replace_category_tag, replace_redirect_category_template};
use crate::is_emergency_stopped;

use super::Status;

#[tracing::instrument]
pub async fn reassignment(
    bot: &Bot,
    from: &String,
    to: &[String],
    discussion_link: &String,
    include_article: bool,
    include_category: bool,
) -> anyhow::Result<Status> {
    let to_page = bot.page(&to[0])?;
    if !to_page.exists().await? {
        move_page(bot, &from, &to[0], format!("BOT: {}", &discussion_link)).await?;
    }

    let mut search = Search::new(format!(r#"insource:"{}""#, from))
        .namespace(vec![
            0,  // 標準名前空間
            14, // Category名前空間
        ])
        .generate(bot);

    let mut done_count = 0;
    while let Some(page) = search.recv().await {
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
                &SaveOptions::summary(&format!("BOT: カテゴリの変更 ({})", &discussion_link)),
            )
            .await;
        done_count += 1;
    }

    Ok(Status::Done { done_count })
}
