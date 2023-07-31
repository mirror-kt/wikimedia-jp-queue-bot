use mwbot::generators::{CategoryMembers, Generator as _, Search};
use mwbot::{Bot, SaveOptions};
use tracing::warn;

use crate::category::{replace_category_tag, replace_redirect_category_template};
use crate::is_emergency_stopped;

use super::Status;

#[tracing::instrument(skip(bot))]
pub async fn remove_category(
    bot: &Bot,
    category: &String,
    discussion_link: &str,
) -> anyhow::Result<Status> {
    let mut search = Search::new(format!(r#"insource:"{}""#, category))
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
                    "BOT: {} カテゴリの削除 ({})",
                    category, discussion_link
                )),
            )
            .await;
        done_count += 1;
    }

    // // カテゴリに所属するページがなくなった場合、即時削除を要請する
    // TODO: 即時削除の方針改定までコメントアウト
    // let mut category_members = CategoryMembers::new(category.to_string()).generate(bot);
    // if category_members.recv().await.is_none() {
    //     let Ok(category_page) = bot.page(category) else {
    //         warn!("Error while getting page: {:?}", category);
    //         return Ok(Status::Done { done_count });
    //     };
    //     if let Err(err) = category_page
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
