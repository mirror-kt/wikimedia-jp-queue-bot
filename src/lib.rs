pub mod action;
pub mod category;
pub mod command;
pub mod generator;
#[cfg(test)]
pub mod test;

use anyhow::Context as _;
use chrono::{FixedOffset, Utc};
use indexmap19::indexmap;
use kuchiki::NodeRef;
use mwbot::parsoid::prelude::*;
use mwbot::parsoid::{Template, Wikicode};
use mwbot::{Bot, Page, SaveOptions};
use tracing::warn;

pub const BOT_NAME: &str = "QueueBot";
pub const QUEUE_PAGE: &str = "プロジェクト:カテゴリ関連/キュー";
pub const EMERGENCY_STOP_PAGE: &str = "プロジェクト:カテゴリ関連/キュー/緊急停止";

pub trait IntoWikicode {
    fn as_wikicode(&self) -> Wikicode;
}

impl IntoWikicode for NodeRef {
    fn as_wikicode(&self) -> Wikicode {
        Wikicode::new_node(&self.to_string())
    }
}

impl IntoWikicode for Wikinode {
    fn as_wikicode(&self) -> Wikicode {
        Wikicode::new(&self.to_string())
    }
}

impl IntoWikicode for Vec<Wikinode> {
    fn as_wikicode(&self) -> Wikicode {
        Wikicode::new(&self.iter().map(|node| node.to_string()).collect::<String>())
    }
}

impl IntoWikicode for String {
    fn as_wikicode(&self) -> Wikicode {
        Wikicode::new_text(self)
    }
}

/// `動作中` と書かれていたら: 動作する (returns false)
/// それ以外(例: `緊急停止`)なら: 止める (returns true)
pub async fn is_emergency_stopped(bot: &Bot) -> bool {
    let Ok(page) = bot.page(EMERGENCY_STOP_PAGE) else {
        return true;
    };
    let Ok(html) = page.html().await.map(|html| html.into_mutable()) else {
        return true;
    };

    let emergency_stopped = html
        .inclusive_descendants()
        .filter_map(|node| node.as_text().map(|text| text.borrow().clone()))
        .skip(1) // Title
        .all(|text| text != "動作中");

    if emergency_stopped {
        warn!("Emergency stop command detected. Stopping...");
    }
    emergency_stopped
}

pub async fn send_success_message(
    queue_page: Page,
    queue: &Section,
    message: &String,
) -> anyhow::Result<Page> {
    let botreq_template = Template::new(
        "BOTREQ",
        &indexmap! {
            "1".to_string() => "完了".to_string(),
        },
    )?;
    let sign_template = Template::new(
        "Eliminator",
        &indexmap! {
            "1".to_string() => BOT_NAME.to_string(),
        },
    )?;
    let current_datetime = get_current_datetime()?;

    queue.append(&botreq_template);
    queue.append(&format!("{} --", message).as_wikicode());
    queue.append(&sign_template);
    queue.append(&current_datetime);

    Ok(queue_page
        .save(
            queue.children().collect::<Vec<_>>().as_wikicode(),
            &SaveOptions::summary(message).section(&queue.section_id().to_string()),
        )
        .await?
        .0)
}

pub async fn send_error_message(
    queue_page: Page,
    queue: &Section,
    message: &String,
) -> anyhow::Result<Page> {
    let botreq_template = Template::new(
        "BOTREQ",
        &indexmap! {
            "1".to_string() => "不受理".to_string(),
        },
    )?;
    let sign_template = Template::new(
        "Eliminator",
        &indexmap! {
            "1".to_string() => BOT_NAME.to_string(),
        },
    )?;
    let current_datetime = get_current_datetime()?;

    queue.append(&botreq_template);
    queue.append(&format!("{} --", message).as_wikicode());
    queue.append(&sign_template);
    queue.append(&current_datetime);

    Ok(queue_page
        .save(
            queue.children().collect::<Vec<_>>().as_wikicode(),
            &SaveOptions::summary(message).section(&queue.section_id().to_string()),
        )
        .await?
        .0)
}

pub async fn send_emergency_stopped_message(
    queue_page: Page,
    queue: &Section,
) -> anyhow::Result<Page> {
    let botreq_template = Template::new(
        "BOTREQ",
        &indexmap! {
            "1".to_string() => "保留".to_string(),
        },
    )?;
    let message = " 緊急停止が作動したためスキップされました. ";
    let sign_template = Template::new(
        "Eliminator",
        &indexmap! {
            "1".to_string() => BOT_NAME.to_string(),
        },
    )?;
    let current_datetime = get_current_datetime()?;

    queue.append(&botreq_template);
    queue.append(&format!("{} --", message).as_wikicode());
    queue.append(&sign_template);
    queue.append(&current_datetime);

    Ok(queue_page
        .save(
            queue.children().collect::<Vec<_>>().as_wikicode(),
            &SaveOptions::summary(message).section(&queue.section_id().to_string()),
        )
        .await?
        .0)
}

fn get_current_datetime() -> anyhow::Result<Wikicode> {
    let current_datetime = Utc::now()
        .with_timezone(&FixedOffset::east_opt(9 * 3600).context("could not parse JST offset")?);

    Ok(current_datetime
        .format("%Y年%m月%d日 %H:%M")
        .to_string()
        .as_wikicode())
}
