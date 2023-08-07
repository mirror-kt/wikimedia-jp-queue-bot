use mwbot::parsoid::prelude::*;
use mwbot::Bot;
use wikimedia_jp_queue_bot::command::{Command, Status};
use wikimedia_jp_queue_bot::{
    send_emergency_stopped_message,
    send_error_message,
    send_success_message,
    QUEUE_PAGE,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let bot = Bot::from_default_config().await?;

    let mut queue_page = bot.page(QUEUE_PAGE)?;
    let queue_html = queue_page.html().await?.into_mutable();

    let sections = queue_html.iter_sections();
    let queues = sections
        .iter()
        .filter(|section| !section.is_pseudo_section())
        .filter(|section| is_prefixed_as_bot(section))
        .filter(|section| !is_done(section))
        .collect::<Vec<_>>();

    for queue in queues {
        let command = Command::parse_command(queue).await;

        let command = match command {
            Ok(command) => command,
            Err(err) => {
                let Ok(page) =
                    send_error_message(queue_page.clone(), queue, &err.to_string()).await
                else {
                    continue;
                };
                queue_page = page;
                continue;
            }
        };

        match command.execute(&bot).await {
            Ok(Status::Done { done_count }) if done_count > 0 => {
                let Ok(page) = send_success_message(
                    queue_page.clone(),
                    queue,
                    &format!("{}件の操作を完了しました", done_count),
                )
                .await
                else {
                    continue;
                };
                queue_page = page;
            }
            Ok(Status::EmergencyStopped) => {
                let Ok(page) = send_emergency_stopped_message(queue_page.clone(), queue).await
                else {
                    continue;
                };
                queue_page = page;
            }
            Err(err) => {
                let Ok(page) =
                    send_error_message(queue_page.clone(), queue, &err.to_string()).await
                else {
                    continue;
                };
                queue_page = page;
            }
            _ => {}
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
