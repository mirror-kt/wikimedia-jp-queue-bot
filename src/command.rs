pub mod reassignment;

use anyhow::{anyhow, Context as _};
use if_chain::if_chain;
use mwbot::parsoid::prelude::*;
use mwbot::Bot;

use self::reassignment::reassignment;

#[derive(Debug)]
pub enum Command {
    /// `from` に直属するすべてのページ(記事とカテゴリ)が `to` に再配属される.
    ///
    /// # コマンド例
    /// - `Bot: [[Category:Name1]]を[[Category:Name2]]へ`
    /// - `Bot: [[Category:Name1]]を[[Category:Name2]]と[[Category:Name3]]へ`
    ReassignmentAll {
        from: String,
        to: Vec<String>,
        discussion_link: String,
    },
    /// `from` に直属するすべての**記事**が `to` に再配属される.
    ///
    /// # コマンド例
    /// - `Bot: (記事) [[Category:Name1]]を[[Category:Name2]]へ`
    /// - `Bot: (記事) [[Category:Name1]]を[[Category:Name2]]と[[Category:Name3]]へ`
    ReassignmentArticle {
        from: String,
        to: Vec<String>,
        discussion_link: String,
    },
    /// `from` に直属するすべての**カテゴリ**が `to` に再配属される.
    ///
    /// # コマンド例
    /// - `Bot: (カテゴリ) [[Category:Name1]]を[[Category:Name2]]へ`
    /// - `Bot: (カテゴリ) [[Category:Name1]]を[[Category:Name2]]と[[Category:Name3]]へ`
    ReassignmentCategory {
        from: String,
        to: Vec<String>,
        discussion_link: String,
    },
    /// `from` に直属するすべてのページから、`from` へのカテゴリ参照を除去する.
    ///
    /// # コマンド例
    /// - `Bot: [[Category:Name1]]を除去`
    RemoveCategory {
        category: String,
        discussion_link: String,
    },
    /// `source` に直属するすべてのページを、`source` に残したまま `dest` にも両属させる.
    ///
    /// # コマンド例
    /// - `Bot: [[Category:Name1]]を[[Category:Name2]]へ複製`
    DuplicateCategory {
        source: String,
        dest: String,
        discussion_link: String,
    },
}

impl Command {
    pub async fn parse_command(section: &Section) -> anyhow::Result<Self> {
        let nodes = section
            .heading()
            .unwrap() // SAFETY: not pseudo_section
            .descendants()
            .skip(1) // Wikicodeのまま入っている
            .collect::<Vec<_>>();

        let bot_prefix = nodes
            .first()
            .and_then(|first| first.as_text())
            .ok_or_else(Self::invalid_command)?;
        let bot_suffix = nodes
            .last()
            .and_then(|last| last.as_text())
            .ok_or_else(Self::invalid_command)?;

        let discussion_link = Self::get_section_discussion(section).await?;

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot:";
            if bot_suffix.borrow().trim() == "へ";
            let nodes = &nodes[1..nodes.len() - 1];
            if let Ok((from, to)) = Self::collect_from_to(nodes);
            if to.iter().all(|x| x.starts_with("Category:"));

            then {
                return Ok(Self::ReassignmentAll { from, to, discussion_link });
            }
        }

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot: (記事)";
            if bot_suffix.borrow().trim() == "へ";
            let nodes = &nodes[1..nodes.len() - 1];
            if let Ok((from, to)) = Self::collect_from_to(nodes);
            if to.iter().all(|x| x.starts_with("Category:"));

            then {
                return Ok(Self::ReassignmentArticle { from, to, discussion_link });
            }
        }

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot: (カテゴリ)";
            if bot_suffix.borrow().trim() == "へ";
            let nodes = &nodes[1..nodes.len() - 1];
            if let Ok((from, to)) = Self::collect_from_to(nodes);
            if to.iter().all(|x| x.starts_with("Category:"));

            then {
                return Ok(Self::ReassignmentCategory { from, to, discussion_link });
            }
        }

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot:";
            if bot_suffix.borrow().trim() == "を除去";
            let nodes = &nodes[1..nodes.len() - 1];
            if nodes.len() == 2;
            if let Some(wikilink) = nodes.first().and_then(|first| first.as_wikilink());
            let category = wikilink.target();
            if category.starts_with("Category:");

            then {
                return Ok(Self::RemoveCategory { category, discussion_link });
            }
        }

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot:";
            if bot_suffix.borrow().trim() == "に複製";
            let nodes = &nodes[1..nodes.len() - 1];
            if let Ok((source, dest)) = Self::collect_from_to(nodes);
            if dest.len() == 1;

            then {
                return Ok(Self::DuplicateCategory { source, dest: dest[0].clone(), discussion_link });
            }
        }

        Err(Self::invalid_command())
    }

    // 〇〇を〇〇((と〇〇)?)+へ をパースする
    fn collect_from_to(nodes: &[Wikinode]) -> anyhow::Result<(String, Vec<String>)> {
        let from = nodes
            .first()
            .and_then(|first| first.as_wikilink())
            .ok_or_else(Self::invalid_command)?
            .target();

        let nodes = &nodes[2..];
        let separator = nodes
            .first()
            .and_then(|first| first.as_text())
            .ok_or_else(Self::invalid_command)?;
        if separator.borrow().trim() != "を" {
            return Err(Self::invalid_command());
        }

        let nodes = &nodes[1..];
        let to = nodes
            // ["カテゴリ名", "と"]で区切る
            // リンクの後に表示文字列が続くので3つずつ区切る
            .chunks(3)
            .map(|chunk| {
                if chunk.len() != 3 {
                    Ok(chunk[0]
                        .as_wikilink()
                        .ok_or_else(Self::invalid_command)?
                        .target())
                } else {
                    if_chain! {
                        if let Some(wikilink) = chunk[0].as_wikilink();
                        let category = wikilink.target();
                        if let Some(separator) = chunk[2].as_text();
                        if separator.borrow().trim() == "と";

                        then {
                            Ok(category)
                        } else {
                            Err(Self::invalid_command())
                        }
                    }
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok((from, to))
    }

    fn invalid_command() -> anyhow::Error {
        anyhow!("コマンド形式が不正です. コマンドを確認し修正してください.")
    }

    // 節に含まれる最初のリンクを取得する
    async fn get_section_discussion(section: &Section) -> anyhow::Result<String> {
        let link = section
            .filter_links()
            .iter()
            .find(|link| !link.target().starts_with("Category:"))
            .cloned()
            .context("議論が行われた場所を示すリンクが必要です.")?;

        Ok(link.target())
    }

    pub async fn execute(&self, bot: &Bot) -> anyhow::Result<Status> {
        match self {
            Self::ReassignmentAll {
                from,
                to,
                discussion_link,
            } => reassignment(bot, from, to, discussion_link, true, true).await,
            Self::ReassignmentArticle {
                from,
                to,
                discussion_link,
            } => reassignment(bot, from, to, discussion_link, true, false).await,
            Self::ReassignmentCategory {
                from,
                to,
                discussion_link,
            } => reassignment(bot, from, to, discussion_link, false, true).await,
            Self::RemoveCategory { .. } => todo!(),
            Self::DuplicateCategory { .. } => todo!(),
        }
    }
}

pub enum Status {
    EmergencyStopped,
    Done { done_count: u32 },
}
