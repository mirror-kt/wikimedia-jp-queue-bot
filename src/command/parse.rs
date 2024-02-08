use std::fmt::Debug;

use anyhow::Context as _;
use mwbot::parsoid::prelude::*;
use mwbot::{Bot, SaveOptions};
use ulid::Ulid;

use crate::db::CommandType;
use crate::replacer::{get_category_replacers, CategoryReplacerList};

pub type Command = super::Command<impl CategoryReplacerList + Debug>;

pub struct Parser {
    bot: Bot,
    prefix: String,
    suffix: String,
    nodes: Vec<Wikinode>,
    discussion_link: String,
    dry_run: bool,
}

impl Parser {
    pub fn new(bot: Bot, section: &Section, dry_run: bool) -> anyhow::Result<Self> {
        let nodes = section
            .heading()
            .context("heading must not be pseudo section")?
            .descendants()
            .skip(1)
            .collect::<Vec<_>>();

        let prefix = nodes
            .first()
            .context("コマンドのプレフィックスが取得できませんんでした")?
            .as_text()
            .context("コマンドのプレフィックスは文字列である必要があります")?
            .borrow()
            .to_string();
        let suffix = nodes
            .last()
            .context("コマンドのサフィックスが取得できませんんでした")?
            .as_text()
            .context("コマンドのサフィックスは文字列である必要があります")?
            .borrow()
            .to_string();
        let discussion_link = section
            .filter_links()
            .into_iter()
            .find(|link| !link.target().starts_with("Category:"))
            .context("議論場所へのリンクがありません")?
            .target();

        Ok(Self {
            bot,
            prefix,
            suffix,
            nodes,
            discussion_link,
            dry_run,
        })
    }

    pub fn parse(self) -> Option<Command> {
        self.parse_reassignment()
            .or_else(|| self.parse_duplicate())
            .or_else(|| self.parse_remove())
    }

    fn parse_reassignment(&self) -> Option<Command> {
        let namespaces = parse_prefix_namespaces(&self.prefix)?;
        if self.suffix != "へ" {
            return None;
        }

        let nodes = self.nodes.get(1..self.nodes.len() - 1)?;
        let (from, to) = collect_from_to(nodes)?;

        let id = Ulid::new();
        let replacers = get_category_replacers(self.bot.clone(), from.clone(), to.clone());
        let save_opts = SaveOptions::summary(&format!(
            "BOT: [[:{}]]から{}へ変更 ([[{}|議論場所]]) (ID: {})",
            &from,
            &to.iter()
                .map(|cat| format!("[[:{}]]", cat))
                .collect::<Vec<_>>()
                .join(","),
            &self.discussion_link,
            &id,
        ));

        Some(Command {
            bot: self.bot.clone(),
            dry_run: self.dry_run,
            id,
            from,
            to,
            discussion_link: self.discussion_link.clone(),
            namespaces,
            replacers,
            save_opts,
            command_type: CommandType::Reassignment,
        })
    }

    fn parse_duplicate(&self) -> Option<Command> {
        let namespaces = parse_prefix_namespaces(&self.prefix)?;
        if self.suffix != "に複製" {
            return None;
        }

        let nodes = self.nodes.get(1..self.nodes.len() - 1)?;
        let (source, mut dest) = collect_from_to(nodes)?;
        dest.push(source.clone());

        let id = Ulid::new();
        let replacers = get_category_replacers(self.bot.clone(), source.clone(), dest.clone());
        let save_opts = SaveOptions::summary(&format!(
            "BOT: [[:{}]]を{}へ複製 ([[{}|議論場所]]) (ID: {})",
            &source,
            &dest
                .iter()
                .map(|cat| format!("[[:{}]]", cat))
                .collect::<Vec<_>>()
                .join(","),
            &self.discussion_link,
            &id,
        ));

        Some(Command {
            bot: self.bot.clone(),
            dry_run: self.dry_run,
            id,
            from: source,
            to: dest,
            discussion_link: self.discussion_link.clone(),
            namespaces,
            replacers,
            save_opts,
            command_type: CommandType::Duplicate,
        })
    }

    fn parse_remove(&self) -> Option<Command> {
        let namespaces = parse_prefix_namespaces(&self.prefix)?;
        if self.suffix != "を除去" {
            return None;
        }

        let nodes = self.nodes.get(1..self.nodes.len() - 1)?;
        let category = nodes.first()?.as_wikilink()?.target();
        if !category.starts_with("Category:") {
            return None;
        }

        let id = Ulid::new();
        let replacers = get_category_replacers(self.bot.clone(), category.clone(), vec![]);
        let save_opts = SaveOptions::summary(&format!(
            "BOT: [[:{}]]を除去 ([[{}|議論場所]]) (ID: {})",
            &category, &self.discussion_link, &id,
        ));

        Some(Command {
            bot: self.bot.clone(),
            dry_run: self.dry_run,
            id,
            from: category,
            to: vec![],
            discussion_link: self.discussion_link.clone(),
            namespaces,
            replacers,
            save_opts,
            command_type: CommandType::Remove,
        })
    }
}

fn parse_prefix_namespaces(prefix: &str) -> Option<Vec<u32>> {
    match prefix.trim() {
        "Bot:" => Some(vec![0, 14]),
        "Bot: (記事)" => Some(vec![0]),
        "Bot: (カテゴリ)" => Some(vec![14]),
        _ => None,
    }
}

const TO_ITEMS_MAX_COUNT: usize = 5;
fn collect_from_to(nodes: &[Wikinode]) -> Option<(String, Vec<String>)> {
    let from = nodes.first()?.as_wikilink()?.target();
    if !from.starts_with("Category:") {
        return None;
    }

    let nodes = nodes.get(2..)?;
    let separator = nodes.first()?.as_text()?;
    if separator.borrow().trim() != "を" {
        return None;
    }

    let nodes = nodes.get(1..)?;
    let to = nodes
        // ["カテゴリ名", "と"]で区切る
        // リンクの後に表示文字列が続くので3つずつ区切る
        .chunks(3)
        .map(|chunk| {
            if chunk.len() != 3 {
                Some(chunk.first()?.as_wikilink()?.target())
            } else {
                let category = chunk.first()?.as_wikilink()?.target();
                if !category.starts_with("Category:") {
                    return None;
                }
                let separator = chunk.get(2)?.as_text()?;
                if separator.borrow().trim() != "と" {
                    return None;
                }

                Some(category)
            }
        })
        .collect::<Option<Vec<_>>>()?;

    if to.is_empty() || to.len() > TO_ITEMS_MAX_COUNT {
        return None;
    }

    Some((from, to))
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use mwbot::parsoid::prelude::*;
    use rstest::rstest;

    use crate::command::parse::Parser;
    use crate::db::CommandType;
    use crate::util::test;

    /// コマンドが正常にパースできることを確認するテスト.
    ///
    /// ```norun
    /// #[case(
    ///     テストするwikitext,
    ///     置換元のカテゴリ,
    ///     置換先のカテゴリ,
    ///     議論場所,
    ///     置換対象の名前空間ID,
    ///     操作種別,
    /// )]
    /// ```
    /// の形式で定義されている.
    #[rstest]
    // ========== 再配属 (全名前空間) ==========
    #[case(
        indoc!{"\
            == Bot: [[:Category:Name1]]を[[:Category:Name2]]へ ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0, 14],
        CommandType::Reassignment,
    )]
    #[case(
    indoc!{"\
            == Bot: [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]へ ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0, 14],
        CommandType::Reassignment,
    )]
    #[case(
        indoc!{"\
            == Bot: [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]と[[:Category:Name4]]と[[:Category:Name5]]と[[:Category:Name6]]へ ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3", "Category:Name4", "Category:Name5", "Category:Name6"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0, 14],
        CommandType::Reassignment,
    )]
    // ========== 再配属 (記事) ==========
    #[case(
        indoc!{"\
            == Bot: (記事) [[:Category:Name1]]を[[:Category:Name2]]へ ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0],
        CommandType::Reassignment,
    )]
    #[case(
        indoc!{"\
            == Bot: (記事) [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]へ ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0],
        CommandType::Reassignment,
    )]
    #[case(
        indoc!{"\
            == Bot: (記事) [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]と[[:Category:Name4]]と[[:Category:Name5]]と[[:Category:Name6]]へ ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3", "Category:Name4", "Category:Name5", "Category:Name6"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0],
        CommandType::Reassignment,
    )]
    // ========== 再配属 (カテゴリ) ==========
    #[case(
        indoc!{"\
            == Bot: (カテゴリ) [[:Category:Name1]]を[[:Category:Name2]]へ ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[14],
        CommandType::Reassignment,
    )]
    #[case(
        indoc!{"\
            == Bot: (カテゴリ) [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]へ ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[14],
        CommandType::Reassignment,
    )]
    #[case(
        indoc!{"\
            == Bot: (カテゴリ) [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]と[[:Category:Name4]]と[[:Category:Name5]]と[[:Category:Name6]]へ ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3", "Category:Name4", "Category:Name5", "Category:Name6"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[14],
        CommandType::Reassignment,
    )]
    // ========== 複製 (全名前空間) ==========
    #[case(
        indoc!{"\
            == Bot: [[:Category:Name1]]を[[:Category:Name2]]に複製 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name1"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0, 14],
        CommandType::Duplicate,
    )]
    #[case(
        indoc!{"\
            == Bot: [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]に複製 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3", "Category:Name1"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0, 14],
        CommandType::Duplicate,
    )]
    #[case(
        indoc!{"\
            == Bot: [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]と[[:Category:Name4]]と[[:Category:Name5]]と[[:Category:Name6]]に複製 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3", "Category:Name4", "Category:Name5", "Category:Name6", "Category:Name1"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0, 14],
        CommandType::Duplicate,
    )]
    // ========== 複製 (記事) ==========
    #[case(
        indoc!{"\
            == Bot: (記事) [[:Category:Name1]]を[[:Category:Name2]]に複製 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name1"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0],
        CommandType::Duplicate,
    )]
    #[case(
        indoc!{"\
            == Bot: (記事) [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]に複製 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3", "Category:Name1"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0],
        CommandType::Duplicate,
    )]
    #[case(
        indoc!{"\
            == Bot: (記事) [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]と[[:Category:Name4]]と[[:Category:Name5]]と[[:Category:Name6]]に複製 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3", "Category:Name4", "Category:Name5", "Category:Name6", "Category:Name1"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0],
        CommandType::Duplicate,
    )]
    // ========== 複製 (カテゴリ) ==========
    #[case(
        indoc!{"\
            == Bot: (カテゴリ) [[:Category:Name1]]を[[:Category:Name2]]に複製 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name1"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[14],
        CommandType::Duplicate,
    )]
    #[case(
        indoc!{"\
            == Bot: (カテゴリ) [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]に複製 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3", "Category:Name1"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[14],
        CommandType::Duplicate,
    )]
    #[case(
        indoc!{"\
            == Bot: (カテゴリ) [[:Category:Name1]]を[[:Category:Name2]]と[[:Category:Name3]]と[[:Category:Name4]]と[[:Category:Name5]]と[[:Category:Name6]]に複製 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &["Category:Name2", "Category:Name3", "Category:Name4", "Category:Name5", "Category:Name6", "Category:Name1"],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[14],
        CommandType::Duplicate,
    )]
    // ========== 除去 (全名前空間) ==========
    #[case(
        indoc!{"\
            == Bot: [[:Category:Name1]]を除去 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &[],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0, 14],
        CommandType::Remove,
    )]
    // ========== 除去 (記事) ==========
    #[case(
        indoc!{"\
            == Bot: (記事) [[:Category:Name1]]を除去 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &[],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[0],
        CommandType::Remove,
    )]
    // ========== 除去 (カテゴリ) ==========
    #[case(
        indoc!{"\
            == Bot: (カテゴリ) [[:Category:Name1]]を除去 ==
            [[プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ|議論]]を参照。 --[[User:Example|Example]] ([[User talk:Example|Talk]])
        "},
        "Category:Name1",
        &[],
        "プロジェクト:カテゴリ関連/議論/yyyy年/mm月dd日#XYZ",
        &[14],
        CommandType::Remove,
    )]
    #[tokio::test]
    async fn test_parse_success(
        #[case] wikitext: &str,
        #[case] from: &str,
        #[case] to: &[&str],
        #[case] discussion_link: &str,
        #[case] namespaces: &[u32],
        #[case] command_type: CommandType,
    ) -> anyhow::Result<()> {
        let bot = test::bot().await;

        let html = bot
            .parsoid()
            .transform_to_html(wikitext)
            .await?
            .into_mutable();
        let sections = html.iter_sections();
        let section = sections
            .into_iter()
            .find(|section| !section.is_pseudo_section())
            .expect("could not get section");

        let parser = Parser::new(bot.clone(), &section, true)?;
        let command = parser.parse().expect("failed to parse command");

        assert!(command.dry_run);
        assert_eq!(command.from, from);
        assert_eq!(command.to, to);
        assert_eq!(command.discussion_link, discussion_link);
        assert_eq!(command.namespaces, namespaces);
        assert_eq!(command.operation_type, operation_type);

        command.execute().await;

        Ok(())
    }
}
