use anyhow::Context;
use mwapi_responses::query;
use mwbot::Bot;

pub async fn move_page(
    bot: &Bot,
    from: impl AsRef<str>,
    to: impl AsRef<str>,
    reason: impl AsRef<str>,
) -> anyhow::Result<()> {
    bot.api()
        .post_with_token(
            "csrf",
            &[
                ("action", "move"),
                ("from", from.as_ref()),
                ("to", to.as_ref()),
                ("noredirect", "true"),
                ("reason", reason.as_ref()),
            ],
        )
        .await?;
    Ok(())
}

#[query(prop = "info", inprop = "protection")]
pub struct InfoResponse {}

pub async fn get_page_info(
    bot: &Bot,
    title: impl Into<String>,
) -> anyhow::Result<InfoResponseItem> {
    let title = title.into();
    let mut resp: InfoResponse = bot.api().query_response([("titles", title)]).await?;
    resp.query
        .pages
        .pop()
        .context("API response returned 0 pages")
}
