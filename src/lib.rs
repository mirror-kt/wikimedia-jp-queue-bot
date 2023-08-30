pub mod action;
pub mod category;
pub mod command;
pub mod config;
pub mod db;
pub mod generator;
#[cfg(test)]
pub mod test;

use std::collections::HashMap;

use chrono::Utc;
use command::OperationStatus;
use indoc::formatdoc;
use kuchiki::NodeRef;
use mwbot::parsoid::prelude::*;
use mwbot::{Bot, Page, SaveOptions};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::Retry;
use tracing::warn;
use ulid::Ulid;

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

const SIGUNATURE: &str = r#"[[User:QueueBot|QueueBot]] <small><span class="plainlinks">([[Special:Contributions/QueueBot|投稿]]/[{{fullurl:Special:Log/delete|user=QueueBot}} 削除]/[{{fullurl:Special:Log/move|user=QueueBot}} 移動])</span></small>"#;
fn get_sigunature() -> String {
    let current_datetime = Utc::now();
    format!(
        "{SIGUNATURE} {} (UTC)",
        current_datetime.format("%Y年%m月%d日 %H:%M")
    )
}

pub async fn send_command_message(
    id: Option<&Ulid>,
    bot: &Bot,
    page: Page,
    section: &Section,
    result: &str,
    message: &str,
    statuses: Option<HashMap<String, OperationStatus>>,
) -> anyhow::Result<Page> {
    let errors = if let Some(statuses) = statuses {
        statuses
            .iter()
            .filter_map(|(page, status)| {
                if let OperationStatus::Error(err) = status {
                    Some((page, err))
                } else {
                    None
                }
            })
            .map(|(page, error)| format!("# {page} - {error}"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        "".to_string()
    };

    let id = if let Some(id) = id {
        format!("(ID: {id})")
    } else {
        "".to_string()
    };

    let content = formatdoc! {"
        {{{{BOTREQ|{result}}}}}{id} - {message}.
        {errors}
        --{sigunature}
        ",
        sigunature = get_sigunature()
    };

    let before_wikicode = bot
        .parsoid()
        .transform_to_wikitext(&section.as_wikicode())
        .await?;
    let after_wikicode = before_wikicode + "\n" + &content;
    let after_html = bot
        .parsoid()
        .transform_to_html(&after_wikicode)
        .await?
        .into_mutable();

    let retry_strategy = ExponentialBackoff::from_millis(5).map(jitter).take(3);
    let (page, _) = Retry::spawn(retry_strategy, || async {
        let page = page.clone();
        page.save(
            after_html.children().collect::<Vec<_>>().as_wikicode(),
            &SaveOptions::summary(&format!("BOT: {message}"))
                .section(&format!("{}", section.section_id())),
        )
        .await
    })
    .await?;

    Ok(page)
}
