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
