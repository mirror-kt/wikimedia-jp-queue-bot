use std::fmt::Display;

use command::OperationStatus;
use indexmap::IndexMap;
use indexmap19::indexmap as indexmap19;
use mwbot::parsoid::prelude::*;
use mwbot::{Bot, Page, SaveOptions};
use tap::Tap as _;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::Retry;
use tracing::warn;
use ulid::Ulid;

use crate::util::{DateTimeProvider, IntoWikicode as _, IterExt as _, UtcDateTimeProvider};

pub mod action;
pub mod category;
pub mod command;
pub mod config;
pub mod db;
pub mod generator;
pub mod util;

pub const BOT_NAME: &str = "QueueBot";
pub const QUEUE_PAGE: &str = "プロジェクト:カテゴリ関連/キュー";
pub const EMERGENCY_STOP_PAGE: &str = "プロジェクト:カテゴリ関連/キュー/緊急停止";

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

fn get_signature<D: DateTimeProvider>(datetime_provider: D) -> Wikicode {
    let current_datetime = datetime_provider.now();

    let signature = Wikicode::new("");
    signature.append(&Wikicode::new_text("--"));
    signature.append(&WikiLink::new(
        "User:QueueBot",
        &Wikicode::new_text("QueueBot"),
    ));

    let small = Wikicode::new_node("small").tap(|small| signature.append(small));

    {
        let span = Wikicode::new_node("span").tap(|span| small.append(span));

        span.as_element()
            .unwrap() // SAFETY: Wikicode created with `Wikicode::new_node` is always an Element
            .attributes
            .borrow_mut()
            .insert("class", "plainlinks".to_string());

        span.append(&Wikicode::new_text("("));
        span.append(&WikiLink::new(
            "Special:Contributions/QueueBot",
            &Wikicode::new_text("投稿"),
        ));
        span.append(&Wikicode::new_text("/"));
        span.append(&ExtLink::new(
            "{{fullurl:Special:Log/delete|user=QueueBot}}",
            &Wikicode::new_text("削除"),
        ));
        span.append(&Wikicode::new_text("/"));
        span.append(&ExtLink::new(
            "{{fullurl:Special:Log/move|user=QueueBot}}",
            &Wikicode::new_text("移動"),
        ));
        span.append(&Wikicode::new_text(")"))
    }

    signature.append(&Wikicode::new_text(&format!(
        " {} (UTC)",
        current_datetime.format("%Y年%m月%d日 %H:%M")
    )));

    signature
}

fn format_message<'i, I: WikinodeIterator, D: DateTimeProvider>(
    wikicode: &'i I,
    id: Option<&Ulid>,
    result: impl Into<String>,
    message: impl Into<String> + Display,
    statuses: Option<IndexMap<String, OperationStatus>>,
    datetime_provider: D,
) -> &'i I {
    let botreq = Template::new(
        "BOTREQ",
        &indexmap19! {
            "1".to_string() => result.into(),
        },
    )
    .expect("unhappened");

    let errors = statuses.map(|statuses| {
        statuses
            .iter()
            .filter_map(|(page, status)| {
                if let OperationStatus::Error(err) = status {
                    Some((page, err))
                } else {
                    None
                }
            })
            .map(|(page, error)| {
                let wikicode = Wikicode::new("");
                let wikilink = WikiLink::new(page, &Wikicode::new_text(page));
                wikicode.append(&wikilink);
                wikicode.append(&Wikicode::new_text(&format!(" - {error}")));

                wikicode
            })
            .collect_to_ol()
    });

    let id = id.map(|id| format!("(ID: {id})").as_wikicode());

    let message = format!(" {message}").as_wikicode();
    let signature = get_signature(datetime_provider);

    wikicode.append(&botreq);
    if let Some(id) = id {
        wikicode.append(&id);
    }
    wikicode.append(&message);
    if let Some(errors) = errors {
        wikicode.append(&errors);
    }
    wikicode.append(&signature);

    wikicode
}

pub async fn send_command_message(
    id: Option<&Ulid>,
    page: Page,
    section: &Section,
    result: impl Into<String>,
    message: impl Into<String> + Display,
    statuses: Option<IndexMap<String, OperationStatus>>,
) -> anyhow::Result<Page> {
    let [result, message] = [result.into(), message.into()];
    let section = format_message(section, id, result, &message, statuses, UtcDateTimeProvider);

    let retry_strategy = ExponentialBackoff::from_millis(5).map(jitter).take(3);
    let (page, _) = Retry::spawn(retry_strategy, || async {
        let page = page.clone();
        page.save(
            section.children().collect::<Vec<_>>().as_wikicode(),
            &SaveOptions::summary(&format!("BOT: {message}"))
                .section(&format!("{}", section.section_id())),
        )
        .await
    })
    .await?;

    Ok(page)
}

#[cfg(test)]
mod test {
    use chrono::{DateTime, TimeZone, Utc};
    use indexmap::{indexmap, IndexMap};
    use indoc::indoc;
    use mwbot::parsoid::Wikicode;
    use ulid::Ulid;

    use crate::command::OperationStatus;
    use crate::util::test;
    use crate::{format_message, get_signature, DateTimeProvider};

    struct CustomDateTimeProvider(DateTime<Utc>);
    impl DateTimeProvider for CustomDateTimeProvider {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[tokio::test]
    async fn test_signature() {
        let bot = test::bot().await;

        let datetime = Utc.with_ymd_and_hms(2023, 10, 17, 0, 0, 0).unwrap();
        let signature = get_signature(CustomDateTimeProvider(datetime));

        let wikitext = bot
            .parsoid()
            .transform_to_wikitext(&signature)
            .await
            .unwrap();

        assert_eq!(
            &wikitext,
            r#"--[[User:QueueBot|QueueBot]]<small><span class="plainlinks">([[Special:Contributions/QueueBot|投稿]]/[{{fullurl:Special:Log/delete|user=QueueBot}} 削除]/[{{fullurl:Special:Log/move|user=QueueBot}} 移動])</span></small> 2023年10月17日 00:00 (UTC)"#
        );
    }

    #[tokio::test]
    async fn test_format_message() {
        let bot = test::bot().await;

        let datetime = Utc.with_ymd_and_hms(2023, 10, 17, 0, 0, 0).unwrap();

        let wikicode = Wikicode::new("");
        format_message(
            &wikicode,
            Some(&Ulid::from_string("01HCZ2CQPV5HW8NJAH6V1Z3KG9").unwrap()),
            "完了",
            "10件の操作が完了しました",
            Some(IndexMap::new()),
            CustomDateTimeProvider(datetime),
        );

        let wikitext = bot
            .parsoid()
            .transform_to_wikitext(&wikicode)
            .await
            .unwrap();

        assert_eq!(
            &wikitext,
            indoc! {r#"
            {{BOTREQ|完了}}(ID: 01HCZ2CQPV5HW8NJAH6V1Z3KG9) 10件の操作が完了しました
        
            --[[User:QueueBot|QueueBot]]<small><span class="plainlinks">([[Special:Contributions/QueueBot|投稿]]/[{{fullurl:Special:Log/delete|user=QueueBot}} 削除]/[{{fullurl:Special:Log/move|user=QueueBot}} 移動])</span></small> 2023年10月17日 00:00 (UTC)"#}
        );
    }

    #[tokio::test]
    async fn test_format_message_with_statuses() {
        let bot = test::bot().await;

        let datetime = Utc.with_ymd_and_hms(2023, 10, 17, 0, 0, 0).unwrap();

        let wikicode = Wikicode::new("");
        format_message(
            &wikicode,
            Some(&Ulid::from_string("01HCZ2CQPV5HW8NJAH6V1Z3KG9").unwrap()),
            "完了",
            "10件の操作が完了しました",
            Some(indexmap! {
                "テスト".to_string() => OperationStatus::Error("これはエラーです".to_string()),
                "テスト2".to_string() => OperationStatus::Error("これはエラーです2".to_string()),
            }),
            CustomDateTimeProvider(datetime),
        );

        let wikitext = bot
            .parsoid()
            .transform_to_wikitext(&wikicode)
            .await
            .unwrap();

        assert_eq!(
            &wikitext,
            indoc! {r#"
            {{BOTREQ|完了}}(ID: 01HCZ2CQPV5HW8NJAH6V1Z3KG9) 10件の操作が完了しました
            
            # [[テスト]] - これはエラーです
            # [[テスト2]] - これはエラーです2
            --[[User:QueueBot|QueueBot]]<small><span class="plainlinks">([[Special:Contributions/QueueBot|投稿]]/[{{fullurl:Special:Log/delete|user=QueueBot}} 削除]/[{{fullurl:Special:Log/move|user=QueueBot}} 移動])</span></small> 2023年10月17日 00:00 (UTC)"#}
        );
    }
}
