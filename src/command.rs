use std::vec;

use anyhow::{anyhow, Context, Error};
use if_chain::if_chain;
use mwbot::parsoid::prelude::*;

#[derive(Debug)]
pub enum Command {
    /// `from` に直属するすべてのページ(記事とカテゴリ)が `to` に再配属される.
    ///
    /// # コマンド例
    /// - `Bot: [[Category:Name1]]を[[Category:Name2]]へ`
    /// - `Bot: [[Category:Name1]]を[[Category:Name2]]と[[Category:Name3]]へ`
    ReassignmentAll { from: String, to: Vec<String> },
    /// `from` に直属するすべての**記事**が `to` に再配属される.
    ///
    /// # コマンド例
    /// - `Bot: (記事) [[Category:Name1]]を[[Category:Name2]]へ`
    /// - `Bot: (記事) [[Category:Name1]]を[[Category:Name2]]と[[Category:Name3]]へ`
    ReassignmentArticle { from: String, to: Vec<String> },
    /// `from` に直属するすべての**カテゴリ**が `to` に再配属される.
    ///
    /// # コマンド例
    /// - `Bot: (カテゴリ) [[Category:Name1]]を[[Category:Name2]]へ`
    /// - `Bot: (カテゴリ) [[Category:Name1]]を[[Category:Name2]]と[[Category:Name3]]へ`
    ReassignmentCategory { from: String, to: Vec<String> },
    /// `from` に直属するすべてのページから、`from` へのカテゴリ参照を除去する.
    ///
    /// # コマンド例
    /// - `Bot: [[Category:Name1]]を除去`
    RemoveCategory { category: String },
    /// `source` に直属するすべてのページを、`source` に残したまま `dest` にも両属させる.
    ///
    /// # コマンド例
    /// - `Bot: [[Category:Name1]]を[[Category:Name2]]へ複製`
    DuplicateCategory { source: String, dest: String },
}

impl Command {
    pub fn parse_command(heading: Heading) -> anyhow::Result<Self> {
        let nodes = heading
            .descendants()
            .skip(1) // Wikicodeのまま入っている
            .collect::<Vec<_>>();

        let bot_prefix = nodes
            .first()
            .ok_or_else(Self::invalid_command)?
            .as_text()
            .ok_or_else(Self::invalid_command)?;
        let bot_suffix = nodes
            .last()
            .ok_or_else(Self::invalid_command)?
            .as_text()
            .ok_or_else(Self::invalid_command)?;

        let nodes: &[Wikinode] = &nodes[1..nodes.len() - 1];

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot:";
            if bot_suffix.borrow().trim() == "へ";

            then {
                let (from, to) = Self::collect_from_to(&nodes)?;
                return Ok(Self::ReassignmentAll { from, to });
            }
        }

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot: (記事)";
            if bot_suffix.borrow().trim() == "へ";

            then {
                let (from, to) = Self::collect_from_to(&nodes)?;
                return Ok(Self::ReassignmentArticle { from, to });
            }
        }

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot: (カテゴリ)";
            if bot_suffix.borrow().trim() == "へ";

            then {
                let (from, to) = Self::collect_from_to(&nodes)?;
                return Ok(Self::ReassignmentCategory { from, to });
            }
        }

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot:";
            if bot_suffix.borrow().trim() == "を除去";
            if nodes.len() == 2;

            then {
                let category = nodes
                    .first()
                    .ok_or_else(Self::invalid_command)?
                    .as_wikilink()
                    .ok_or_else(Self::invalid_command)?
                    .target();
                return Ok(Self::RemoveCategory { category });
            }
        }

        if_chain! {
            if bot_prefix.borrow().trim() == "Bot:";
            if bot_suffix.borrow().trim() == "に複製";

            then {
                let (source, dest) = Self::collect_from_to(&nodes)?;
                if dest.len() != 1 {
                    return Err(Self::invalid_command());
                }

                return Ok(Self::DuplicateCategory { source, dest: dest[0].clone() });
            }
        }

        Err(Self::invalid_command())
    }

    // 〇〇を〇〇((と〇〇)?)+へ をパースする
    fn collect_from_to(nodes: &[Wikinode]) -> anyhow::Result<(String, Vec<String>)> {
        let from = nodes
            .first()
            .ok_or_else(Self::invalid_command)?
            .as_wikilink()
            .ok_or_else(Self::invalid_command)?
            .target();

        let nodes = &nodes[2..];
        let separator = nodes
            .first()
            .ok_or_else(Self::invalid_command)?
            .as_text()
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
}
