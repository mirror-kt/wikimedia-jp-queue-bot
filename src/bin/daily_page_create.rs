use anyhow::Context as _;
use chrono::{Days, FixedOffset, Utc};
use indoc::formatdoc;
use mwbot::{Bot, SaveOptions};
use tracing::info;

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

    // 翌日分のページを作成
    let page_name = format!("プロジェクト:カテゴリ関連/議論/{year}年/{month}月{day}日");

    let page = bot.page(&page_name)?;
    if page.exists().await? {
        info!("page {} already exists", page.title());
        return Ok(());
    }

    let page_content = formatdoc! {r#"
        <noinclude>{{{{プロジェクト:カテゴリ関連/議論/見出し|年={year}|月={month}|日={day}}}}}

        __NEWSECTIONLINK__

        == [[{page_name}|カテゴリ]] ==
        </noinclude><includeonly>== [[{page_name}|{month}月{day}日]] ==</includeonly>
        <!-- 新規の議論は一番下に記入してください。 -->
    "#};

    page.save(page_content, &SaveOptions::summary("BOT: 議論ページの作成"))
        .await?;

    Ok(())
}
