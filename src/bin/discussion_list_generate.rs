use chrono::{Datelike, NaiveDate, Utc};
use futures_util::{stream, StreamExt, TryStreamExt};
use indexmap::IndexMap;
use mwbot::parsoid::prelude::*;
use mwbot::{Bot, SaveOptions};
use queuebot::config::{self, DiscussionSummaryIconBindings, OnWikiConfig};
use queuebot::util::{IntoWikicode, ListExt};
use tap::{Pipe, Tap};
use tracing::info;

const DISCUSSION_CLOSE_TEMPLATES: &[&str] =
    &["Template:古い話題のはじめ", "Template:古い話題のおわり"];
const CONFIG_JSON_URL: &str =
    "https://ja.wikipedia.org/wiki/利用者:QueueBot/config.json?action=raw";
const OUTPUT_PAGE: &str = "プロジェクト:カテゴリ関連/議論/アクティブな議論一覧";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let bot = Bot::from_default_config().await?;

    let on_wiki_config = reqwest::get(CONFIG_JSON_URL)
        .await?
        .json::<OnWikiConfig>()
        .await?;

    let discussion_summary = stream::iter(Utc::now().date_naive().iter_days().rev().take(30))
        .then(|date| {
            let bot = bot.clone();
            let bindings = on_wiki_config.discussion_summary_icon_bindings.as_slice();
            async move { get_active_discussions(&bot, bindings, &date).await }
        })
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .flatten()
        .rev()
        .collect::<Vec<_>>()
        .into_wikicode();

    let output = bot.page(OUTPUT_PAGE)?;
    output
        .save(
            discussion_summary,
            &SaveOptions::summary("BOT: アクティブな議論一覧を更新"),
        )
        .await?;

    Ok(())
}

#[tracing::instrument(skip(bot, binds))]
async fn get_active_discussions(
    bot: &Bot,
    binds: &[DiscussionSummaryIconBindings],
    date: &NaiveDate,
) -> anyhow::Result<Option<Wikicode>> {
    let page_title = format!(
        "プロジェクト:カテゴリ関連/議論/{}年/{}月{}日",
        date.year(),
        date.month(),
        date.day()
    );
    let page = bot.page(&page_title)?;
    let html = page.html().await?.into_mutable();

    info!("Processing");

    let active_discussion_sections = html
        .iter_sections()
        .into_iter()
        .filter_map(|section| {
            let heading = section.heading()?;
            if heading.level() == 2 {
                Some((section, heading.text_contents()))
            } else {
                None
            }
        })
        .filter_map(|(section, section_name)| {
            if section
                .filter_templates()
                .ok()?
                .iter()
                .any(|template| DISCUSSION_CLOSE_TEMPLATES.contains(&&*template.name()))
            {
                None
            } else {
                Some((section, section_name))
            }
        })
        .map(|(section, section_name)| {
            process_discussion(binds, &page_title, &section, &section_name)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    if active_discussion_sections.is_empty() {
        return Ok(None);
    }
    let active_discussion_sections = active_discussion_sections.into_iter().collect_to_ol();

    let wikicode = Wikicode::new("");
    let heading = Heading::new(
        2,
        &WikiLink::new(
            &page_title,
            &Wikicode::new_text(&format!("{}月{}日", date.month(), date.day())),
        ),
    )?;
    wikicode.append(&heading);
    wikicode.append(&active_discussion_sections);

    Ok(Some(wikicode))
}

#[tracing::instrument(skip(binds, section))]
fn process_discussion(
    binds: &[DiscussionSummaryIconBindings],
    page_title: &str,
    section: &Section,
    section_name: &str,
) -> anyhow::Result<Wikicode> {
    let valid_votes = section
        .filter_templates()?
        .into_iter()
        .filter_map(|t| normalize_vote(binds, t))
        .fold(IndexMap::new(), |mut acc, value| {
            let count = acc.entry(value).or_insert(0);
            *count += 1;
            acc
        });

    info!(votes = ?valid_votes);

    let wikicode = Wikicode::new("");
    wikicode.append(
        &WikiLink::new(
            &format!("{page_title}#{section_name}"),
            &Wikicode::new(section_name),
        )
        .pipe(|link| Wikicode::new_node("b").tap(|b| b.append(&link))),
    );
    wikicode.append(&Wikicode::new("\n"));

    for (vote, count) in valid_votes {
        wikicode.append::<Template>(&vote.try_into()?);
        wikicode.append(&Wikicode::new_text(&format!("({count})")));
    }

    Ok(wikicode)
}

fn normalize_vote(
    binds: &[DiscussionSummaryIconBindings],
    template: Template,
) -> Option<&config::Template> {
    binds
        .iter()
        .find(|bind| {
            bind.main.matches(&template)
                || bind.alternatives.iter().any(|alt| alt.matches(&template))
        })
        .map(|bind| &bind.main)
}
