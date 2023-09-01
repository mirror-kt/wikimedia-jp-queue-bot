use anyhow::Context as _;
use chrono::{Days, FixedOffset, Utc};
use mwbot::{Bot, SaveOptions};
use tracing::info;

const PAGE_TEMPLATE: &str = "プロジェクト:カテゴリ関連/議論/日別ページ雛形";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let bot = Bot::from_default_config().await?;

    let today = Utc::now()
        .with_timezone(&FixedOffset::east_opt(9 * 3600).context("could not parse JST offset")?);
    let tomorrow = today.checked_add_days(Days::new(1)).context("overflowed")?;

    let year = tomorrow.format("%Y");
    let month = tomorrow.format("%-m"); // without 0-padding
    let day = tomorrow.format("%-d"); // without 0-padding

    let page_template = bot
        .page(PAGE_TEMPLATE)
        .context("could not get template page")?
        .wikitext()
        .await?;

    // 翌日分のページを作成
    let page_name = format!("プロジェクト:カテゴリ関連/議論/{year}年/{month}月{day}日");

    let page_content = page_template
        .replace("{year}", &year.to_string())
        .replace("{month}", &month.to_string())
        .replace("{day}", &day.to_string())
        .replace("{page_name}", &page_name);

    let page = bot.page(&page_name)?;
    if page.exists().await? {
        info!("page {} already exists", page.title());
        return Ok(());
    }

    page.save(page_content, &SaveOptions::summary("BOT: 議論ページの作成"))
        .await?;

    Ok(())
}
