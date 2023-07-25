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

    // 翌日分のページを作成
    let page = bot.page(&format!(
        "プロジェクト:カテゴリ関連/議論/{}/{}",
        &tomorrow.format("%Y年"),
        &tomorrow.format("%m月%d日"),
    ))?;
    if page.exists().await? {
        info!("page {} already exists", page.title());
        return Ok(());
    }

    let page_content = formatdoc! {r"
        == {} ==
        <noinclude> {{{{Purge}}}} - </noinclude>{{{{カテゴリ関連/議論/ログ日付|date={}}}}}
        <!-- 新規の議論は一番下につけたしてください。 -->
        ",
        tomorrow.format("%m月%d日"),
        tomorrow.format("%Y-%m-%d"),
    };

    page.save(page_content, &SaveOptions::summary("BOT: 議論ページの作成"))
        .await?;

    Ok(())
}
