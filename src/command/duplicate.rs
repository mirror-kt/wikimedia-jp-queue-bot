use mwbot::generators::{Generator as _, Search};
use mwbot::{Bot, SaveOptions};
use tracing::warn;

use crate::category::{replace_category_tag, replace_redirect_category_template};
use crate::is_emergency_stopped;

use super::Status;

#[tracing::instrument(skip(bot))]
pub async fn duplicate_category(
    bot: &Bot,
    source: &String,
    dest: &String,
    discussion_link: &str,
) -> anyhow::Result<Status> {
    let mut search = Search::new(format!(r#"insource:"{}""#, source))
        .namespace(vec![
            0,  // 標準名前空間
            14, // Category名前空間
        ])
        .generate(bot);
    let to = &[source.to_string(), dest.to_string()];

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

        replace_category_tag(&html, source, to);
        replace_redirect_category_template(&html, source, to);

        let _ = page
            .save(
                html,
                &SaveOptions::summary(&format!(
                    "BOT: {} カテゴリを {} カテゴリに複製 ([[{}]])",
                    source, dest, discussion_link
                )),
            )
            .await;
        done_count += 1;
    }
    Ok(Status::Done { done_count })
}
