use mwbot::parsoid::prelude::*;
use mwbot::Bot;
use wikimedia_jp_queue_bot::command::Command;


const QUEUE_PAGE: &str = "利用者:Misato_Kano/sandbox/プロジェクト:カテゴリ関連/キュー";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let bot = Bot::from_default_config().await?;

    let queue_page = bot.page(QUEUE_PAGE)?;
    let queue_html = queue_page.html().await?.into_mutable();

    let sections = queue_html.iter_sections();
    let queues = sections
        .iter()
        .filter(|section| !section.is_pseudo_section())
        .filter(|section| is_prefixed_as_bot(section))
        .collect::<Vec<_>>();

    for queue in queues {
        let command = Command::parse_command(queue).await;
        dbg!(&command);

        // if let Err(error) = command {
        //     queue_page = send_error_message(error, queue_page, queue).await?;
        // }
    }

    Ok(())
}

/// セクション名は `Bot:` で始まるか
fn is_prefixed_as_bot(section: &Section) -> bool {
    let heading = section.heading().unwrap(); // SAFETY: not pseudo section
    let prefix_node = heading.descendants().nth(1).unwrap();
    if let Some(prefix) = prefix_node.as_text() {
        prefix.borrow().starts_with("Bot:")
    } else {
        false
    }
}
