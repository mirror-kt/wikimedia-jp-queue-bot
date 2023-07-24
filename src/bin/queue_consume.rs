use mwbot::parsoid::map::IndexMap;
use mwbot::parsoid::prelude::*;
use mwbot::{Bot, SaveOptions};
use wikimedia_jp_queue_bot::command::Command;
use wikimedia_jp_queue_bot::IntoWikicode as _;

const BOT_NAME: &str = "MirrorKtBot";

const EMERGENCY_STOP_PAGE: &str = "利用者:Misato_Kano/sandbox/緊急停止テスト2";
const QUEUE_PAGE: &str = "利用者:Misato_Kano/sandbox/プロジェクト:カテゴリ関連/キュー";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let bot = Bot::from_default_config().await?;

    let mut queue_page = bot.page(QUEUE_PAGE)?;
    let queue_html = queue_page.html().await?.into_mutable();

    let sections = queue_html.iter_sections();
    let queues = sections
        .iter()
        .filter(|section| is_prefixed_as_bot(section))
        .collect::<Vec<_>>();

    for queue in queues {
        let title = get_section_title(queue).unwrap(); // SAFETY(unwrap): is_prefixed_as_bot
        let summary = get_section_summary(bot.parsoid(), queue).await?;

        let command = Command::parse_command(queue.heading().unwrap());
        dbg!(command);

        // let botreq_template = Template::new_simple("BOTREQ");
        // let _ = botreq_template.set_param("1", "完了");

        // let done_text = format!("{} 件の記事を移動しました - ", "test").into_wikicode();

        // let sign_template = Template::new_simple("Eliminator");
        // let _ = sign_template.set_param("1", BOT_NAME);

        // queue.append(&botreq_template);
        // queue.append(&done_text);
        // queue.append(&sign_template);

        // queue_page = queue_page
        //     .save(
        //         queue.children().collect::<Vec<_>>().into_wikicode(),
        //         &SaveOptions::summary("完了: test").section(&queue.section_id().to_string()),
        //     )
        //     .await?
        //     .0;
    }

    Ok(())
}

/// セクション名は `Bot:` で始まるか
fn is_prefixed_as_bot(section: &Section) -> bool {
    get_section_title(section).map_or(false, |title| title.starts_with("Bot:_"))
}

/// セクション名を取得する
fn get_section_title(section: &Section) -> Option<String> {
    let heading = section.heading()?;
    let element_data = heading.as_element()?;
    let attributes = element_data.attributes.borrow();

    let title = attributes.get("id");
    title.map(|title| title.to_owned())
}

async fn get_section_summary(parsoid: &ParsoidClient, section: &Section) -> anyhow::Result<String> {
    let summary = section
        .children()
        .skip(2) // Heading + \n
        .collect::<Vec<_>>()
        .into_wikicode();
    let summary_wikitext = parsoid.transform_to_wikitext(&summary).await?;

    Ok(summary_wikitext)
}

/// `動作中` と書かれていたら: 動作する (returns false)
/// それ以外(例: `緊急停止`)なら: 止める (returns true)
async fn is_emergency_stopped(bot: &Bot) -> anyhow::Result<bool> {
    let page = bot.page(EMERGENCY_STOP_PAGE)?;
    let html = page.html().await?.into_mutable();

    Ok(html
        .inclusive_descendants()
        .filter_map(|node| node.as_text().map(|text| text.borrow().clone()))
        .skip(1) // Title
        .all(|text| text != "動作中"))
}
