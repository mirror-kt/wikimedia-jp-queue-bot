use mwbot::parsoid::prelude::*;
use mwbot::Bot;
use tracing::warn;
use wikimedia_jp_queue_bot::command::parse::Parser;
use wikimedia_jp_queue_bot::command::CommandStatus;
use wikimedia_jp_queue_bot::config::load_config;
use wikimedia_jp_queue_bot::{db, send_command_message, QUEUE_PAGE};

macro_rules! send_command_message {
    ($id:expr, $queue_page:expr, $queue:expr, $result:expr, $message:expr, $statuses:expr) => {
        match send_command_message(
            $id,
            $queue_page.clone(),
            $queue,
            $result,
            $message,
            $statuses,
        )
        .await
        {
            Ok(page) => {
                $queue_page = page;
            }
            Err(err) => {
                warn!(page = QUEUE_PAGE, ?err, "could not save command log");
                continue;
            }
        }
    };
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let bot = Bot::from_default_config().await?;
    let config = load_config()?;

    db::init(&config.mysql).await?;

    let mut queue_page = bot.page(QUEUE_PAGE)?;
    let queue_html = queue_page.html().await?.into_mutable();

    let sections = queue_html.iter_sections();
    let queues = sections
        .into_iter()
        .filter(|section| !section.is_pseudo_section())
        .filter(is_prefixed_as_bot)
        .filter(|section| !is_done(section))
        .collect::<Vec<_>>();

    for queue in queues {
        let parser = match Parser::new(bot.clone(), &queue, false) {
            Ok(command) => command,
            Err(err) => {
                warn!(?err, "parsing error occurred");
                send_command_message!(None, queue_page, &queue, "不受理", &err.to_string(), None);
                continue;
            }
        };
        let Some(command) = parser.parse() else {
            let section_name = queue
                .heading()
                .unwrap() // SAFETY: pseudo checked
                .text_contents();
            warn!(section_name = ?section_name, "Invalid command format");
            send_command_message!(
                None,
                queue_page,
                &queue,
                "不受理",
                "不明なコマンドです",
                None
            );
            continue;
        };

        match command.execute().await {
            CommandStatus::Done { id, statuses } => {
                send_command_message!(
                    Some(&id),
                    queue_page,
                    &queue,
                    "完了",
                    &format!("{}件の操作を完了しました", statuses.len()),
                    Some(statuses)
                );
            }
            CommandStatus::EmergencyStopped => {
                send_command_message!(None, queue_page, &queue, "保留", "緊急停止しました", None);
                continue;
            }
            CommandStatus::Error {
                id,
                statuses,
                message,
            } => {
                send_command_message!(
                    Some(&id),
                    queue_page,
                    &queue,
                    "中止",
                    &message,
                    Some(statuses)
                );
            }
            CommandStatus::Skipped => {
                // do nothing
            }
        }
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

/// そのキューは既に実行済み({{BOTREQ|完了}}が貼られている)か
fn is_done(section: &Section) -> bool {
    let children = section.inclusive_descendants();
    let mut templates = children
        .flat_map(|child| child.filter_templates())
        .flatten();

    templates.any(|template| {
        template.name() == "Template:BOTREQ" && template.param("1") == Some("完了".to_string())
    })
}
