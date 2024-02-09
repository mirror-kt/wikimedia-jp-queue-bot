use anyhow::Context;
use mwapi_responses::query;
use mwbot::Bot;

#[query(prop = "info", inprop = "protection")]
pub struct InfoResponse {}

pub async fn get_page_info(
    bot: &Bot,
    title: impl Into<String>,
) -> anyhow::Result<InfoResponseItem> {
    let title = title.into();
    let mut resp: InfoResponse = mwapi_responses::query_api(bot.api(), [("titles", title)]).await?;
    resp.query
        .pages
        .pop()
        .context("API response returned 0 pages")
}
