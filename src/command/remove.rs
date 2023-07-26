use mwbot::generators::{Generator as _, Search};
use mwbot::Bot;
use tracing::warn;

use crate::is_emergency_stopped;

use super::Status;

pub async fn remove_category(
    bot: &Bot,
    category: &String,
    _discussion_link: &str,
) -> anyhow::Result<Status> {
    let mut search = Search::new(format!(r#"insource:"{}""#, &category))
        .namespace(vec![
            0,  // 標準名前空間
            14, // Category名前空間
        ])
        .generate(bot);

    let done_count = 0;
    while let Some(page) = search.recv().await {
        if is_emergency_stopped(bot).await {
            return Ok(Status::EmergencyStopped);
        }

        let Ok(page) = page else {
            warn!("Error while searching: {:?}", page);
            continue;
        };

        let Ok(wikitext) = page.wikitext().await else {
            warn!("Error while getting wikitext: {:?}", page);
            continue;
        };
        let _replaced = wikitext.replace(&format!("[[{}]]", category), "");
    }
    Ok(Status::Done { done_count })
}
