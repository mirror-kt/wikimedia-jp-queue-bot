pub mod action;
pub mod command;
pub mod consume;
#[cfg(test)]
pub mod test;

use indexmap19::indexmap;
use kuchiki::NodeRef;
use mwbot::parsoid::prelude::*;
use mwbot::parsoid::{Template, Wikicode};
use mwbot::{Bot, Page, SaveOptions};
use tracing::warn;

const BOT_NAME: &str = "MirrorKtBot";
const EMERGENCY_STOP_PAGE: &str = "利用者:Misato_Kano/sandbox/緊急停止テスト2";

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

pub async fn send_error_message(
    error: impl ToString,
    queue_page: Page,
    queue: &Section,
) -> anyhow::Result<Page> {
    let botreq_template = Template::new(
        "BOTREQ",
        &indexmap! {
            "1".to_string() => "不受理".to_string(),
        },
    )?;
    let err_message = format!(" {} ", error.to_string()).as_wikicode();
    let sign_template = Template::new(
        "Eliminator",
        &indexmap! {
            "1".to_string() => BOT_NAME.to_string(),
        },
    )?;
    queue.append(&botreq_template);
    queue.append(&err_message);
    queue.append(&sign_template);

    Ok(queue_page
        .save(
            queue.children().collect::<Vec<_>>().as_wikicode(),
            &SaveOptions::summary(&error.to_string()).section(&queue.section_id().to_string()),
        )
        .await?
        .0)
}
