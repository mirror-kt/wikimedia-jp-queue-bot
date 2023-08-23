pub mod duplicate;
pub mod reassignment;
pub mod remove;

use std::cell::RefCell;

use anyhow::{anyhow, Context as _};
use if_chain::if_chain;
use mwbot::parsoid::prelude::*;
use mwbot::Bot;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use self::duplicate::duplicate_category;
use self::reassignment::reassignment;
use self::remove::remove_category;
use crate::config::{MySqlConfig, QueueBotConfig};
use crate::db::{store_command, CommandType};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
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

        Self::try_parse_reassignment_all_command(bot_prefix, bot_suffix, &nodes, &discussion_link)
            .or_else(|| {
                Self::try_parse_reassignment_article_command(
                    bot_prefix,
                    bot_suffix,
                    &nodes,
                    &discussion_link,
                )
            })
            .or_else(|| {
                Self::try_parse_reassignment_category_command(
                    bot_prefix,
                    bot_suffix,
                    &nodes,
                    &discussion_link,
                )
            })
            .or_else(|| {
                Self::try_parse_remove_category_command(
                    bot_prefix,
                    bot_suffix,
                    &nodes,
                    &discussion_link,
                )
            })
            .or_else(|| {
                Self::try_parse_duplicate_category_command(
                    bot_prefix,
                    bot_suffix,
                    &nodes,
                    &discussion_link,
                )
            })
            .ok_or_else(Self::invalid_command)
    }

    fn try_parse_reassignment_all_command(
        bot_prefix: &RefCell<String>,
        bot_suffix: &RefCell<String>,
        nodes: &[Wikinode],
        discussion_link: &String,
    ) -> Option<Self> {
        if_chain! {
            if bot_prefix.borrow().trim() == "Bot:";
            if bot_suffix.borrow().trim() == "へ";
            let nodes = &nodes[1..nodes.len() - 1];
            if let Ok((from, to)) = Self::collect_from_to(nodes);
            if !to.is_empty();
            if to.len() <= 5;
            if to.iter().all(|x| x.starts_with("Category:"));

            then {
                return Some(Self::ReassignmentAll { from, to, discussion_link: discussion_link.to_owned() });
            }
        }

        None
    }

    fn try_parse_reassignment_article_command(
        bot_prefix: &RefCell<String>,
        bot_suffix: &RefCell<String>,
        nodes: &[Wikinode],
        discussion_link: &String,
    ) -> Option<Self> {
        if_chain! {
            if bot_prefix.borrow().trim() == "Bot: (記事)";
            if bot_suffix.borrow().trim() == "へ";
            let nodes = &nodes[1..nodes.len() - 1];
            if let Ok((from, to)) = Self::collect_from_to(nodes);
            if !to.is_empty();
            if to.len() <= 5;
            if to.iter().all(|x| x.starts_with("Category:"));

            then {
                return Some(Self::ReassignmentArticle { from, to, discussion_link: discussion_link.to_owned() });
            }
        }

        None
    }

    fn try_parse_reassignment_category_command(
        bot_prefix: &RefCell<String>,
        bot_suffix: &RefCell<String>,
        nodes: &[Wikinode],
        discussion_link: &String,
    ) -> Option<Self> {
        if_chain! {
            if bot_prefix.borrow().trim() == "Bot: (カテゴリ)";
            if bot_suffix.borrow().trim() == "へ";
            let nodes = &nodes[1..nodes.len() - 1];
            if let Ok((from, to)) = Self::collect_from_to(nodes);
            if !to.is_empty();
            if to.len() <= 5;
            if to.iter().all(|x| x.starts_with("Category:"));

            then {
                return Some(Self::ReassignmentCategory { from, to, discussion_link: discussion_link.to_owned() });
            }
        }

        None
    }

    fn try_parse_remove_category_command(
        bot_prefix: &RefCell<String>,
        bot_suffix: &RefCell<String>,
        nodes: &[Wikinode],
        discussion_link: &String,
    ) -> Option<Self> {
        if_chain! {
            if bot_prefix.borrow().trim() == "Bot:";
            if bot_suffix.borrow().trim() == "を除去";
            let nodes = &nodes[1..nodes.len() - 1];
            if nodes.len() == 2;
            if let Some(wikilink) = nodes.first().and_then(|first| first.as_wikilink());
            let category = wikilink.target();
            if category.starts_with("Category:");

            then {
                return Some(Self::RemoveCategory { category, discussion_link: discussion_link.to_owned() });
            }
        }

        None
    }

    fn try_parse_duplicate_category_command(
        bot_prefix: &RefCell<String>,
        bot_suffix: &RefCell<String>,
        nodes: &[Wikinode],
        discussion_link: &String,
    ) -> Option<Self> {
        if_chain! {
            if bot_prefix.borrow().trim() == "Bot:";
            if bot_suffix.borrow().trim() == "に複製";
            let nodes = &nodes[1..nodes.len() - 1];
            if let Ok((source, dest)) = Self::collect_from_to(nodes);
            if dest.len() == 1;
            if dest[0].starts_with("Category:");

            then {
                return Some(Self::DuplicateCategory { source, dest: dest[0].clone(), discussion_link: discussion_link.to_owned() });
            }
        }

        None
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
            .ok_or_else(Self::discussion_link_notfound)?;

        Ok(link.target())
    }

    fn discussion_link_notfound() -> anyhow::Error {
        anyhow!("議論が行われた場所を示すリンクが必要です.")
    }

    pub async fn execute(&self, bot: &Bot, config: &QueueBotConfig) -> anyhow::Result<Status> {
        let id = Ulid::new();
        self.insert_db(&id, &config.mysql).await?;

        match self {
            Self::ReassignmentAll {
                from,
                to,
                discussion_link,
            } => reassignment(bot, config, &id, from, to, discussion_link, true, true).await,
            Self::ReassignmentArticle {
                from,
                to,
                discussion_link,
            } => reassignment(bot, config, &id, from, to, discussion_link, true, false).await,
            Self::ReassignmentCategory {
                from,
                to,
                discussion_link,
            } => reassignment(bot, config, &id, from, to, discussion_link, false, true).await,
            Self::RemoveCategory {
                category,
                discussion_link,
            } => remove_category(bot, config, &id, category, discussion_link).await,
            Self::DuplicateCategory {
                source,
                dest,
                discussion_link,
            } => duplicate_category(bot, config, &id, source, dest, discussion_link).await,
        }
    }

    async fn insert_db(&self, id: &Ulid, config: &MySqlConfig) -> anyhow::Result<()> {
        let params = serde_json::to_value(self).context("could not serialize command")?;

        let command_type = match *self {
            Self::ReassignmentAll { .. } => CommandType::ReassignmentAll,
            Self::ReassignmentArticle { .. } => CommandType::ReassignmentArticle,
            Self::ReassignmentCategory { .. } => CommandType::ReassignmentCategory,
            Self::RemoveCategory { .. } => CommandType::RemoveCategory,
            Self::DuplicateCategory { .. } => CommandType::DuplicateCategory,
        };

        store_command(config, id, command_type, params).await
    }
}

pub enum Status {
    EmergencyStopped,
    Done { id: Ulid, done_count: u32 },
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use mwbot::parsoid::prelude::*;

    use super::Command;
    use crate::test;

    #[tokio::test]
    async fn test_parse_command() {
        let bot = test::bot().await;

        let wikitext = indoc! {"
            == Bot: [[:Category:Example]]を[[:Category:Example2]]へ ==
            [[利用者:Misato Kano]]を参照

            == Bot: [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]へ ==
            [[利用者:Misato Kano]]を参照

            == Bot: (記事) [[:Category:Name1]]を[[:Category:Name2]]へ ==
            [[利用者:Misato Kano]]を参照

            == Bot: (カテゴリ) [[:Category:Name1]]を[[:Category:Name2]]へ ==
            [[利用者:Misato Kano]]を参照

            == Bot: [[:Category:Name1]]を除去 ==
            [[利用者:Misato Kano]]を参照

            == Bot: [[:Category:Name1]]を[[:Category:Name2]]に複製 ==
            [[利用者:Misato Kano]]を参照

            == Bot: Category:ExampleをCategory:Example2へ ==
            [[利用者:Misato Kano]]を参照

            == Bot: [[:Category:Example]]を[[:Category:Example2]]へ ==
            no link
        "};

        let html = bot
            .parsoid()
            .transform_to_html(wikitext)
            .await
            .unwrap()
            .into_mutable();

        let sections = html.iter_sections();

        let result = futures_util::future::join_all(
            sections
                .iter()
                .filter(|section| !section.is_pseudo_section())
                .map(Command::parse_command)
                .collect::<Vec<_>>(),
        )
        .await;

        let command1 = result[0].as_ref().unwrap();
        assert_eq!(
            *command1,
            Command::ReassignmentAll {
                from: "Category:Example".to_string(),
                to: vec!["Category:Example2".to_string()],
                discussion_link: "利用者:Misato Kano".to_string(),
            }
        );

        let command2 = result[1].as_ref().unwrap();
        assert_eq!(
            *command2,
            Command::ReassignmentAll {
                from: "Category:Name1".to_string(),
                to: vec!["Category:Name2".to_string(), "Category:Name3".to_string()],
                discussion_link: "利用者:Misato Kano".to_string(),
            }
        );

        let command3 = result[2].as_ref().unwrap();
        assert_eq!(
            *command3,
            Command::ReassignmentArticle {
                from: "Category:Name1".to_string(),
                to: vec!["Category:Name2".to_string()],
                discussion_link: "利用者:Misato Kano".to_string(),
            }
        );

        let command4 = result[3].as_ref().unwrap();
        assert_eq!(
            *command4,
            Command::ReassignmentCategory {
                from: "Category:Name1".to_string(),
                to: vec!["Category:Name2".to_string()],
                discussion_link: "利用者:Misato Kano".to_string(),
            }
        );

        let command5 = result[4].as_ref().unwrap();
        assert_eq!(
            *command5,
            Command::RemoveCategory {
                category: "Category:Name1".to_string(),
                discussion_link: "利用者:Misato Kano".to_string()
            }
        );

        let command6 = result[5].as_ref().unwrap();
        assert_eq!(
            *command6,
            Command::DuplicateCategory {
                source: "Category:Name1".to_string(),
                dest: "Category:Name2".to_string(),
                discussion_link: "利用者:Misato Kano".to_string()
            }
        );

        let command7 = result[6].as_ref().map_err(|err| err.to_string()).err();
        assert_eq!(
            command7,
            Some("コマンド形式が不正です. コマンドを確認し修正してください.".to_string())
        );

        let command8 = result[7].as_ref().map_err(|err| err.to_string()).err();
        assert_eq!(
            command8,
            Some("議論が行われた場所を示すリンクが必要です.".to_string())
        );
    }
}
