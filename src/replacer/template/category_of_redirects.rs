use indexmap::IndexMap;
use mwbot::parsoid::prelude::*;
use regex::Regex;
use tracing::warn;

use crate::replacer::CategoryReplacer;

#[derive(Debug, Clone)]
pub struct CategoryOfRedirectsReplacer {
    from: String,
    to: Vec<String>,
}

impl CategoryOfRedirectsReplacer {
    pub fn new(from: String, to: Vec<String>) -> Self {
        Self { from, to }
    }
}

impl CategoryReplacer for CategoryOfRedirectsReplacer {
    async fn replace(&self, html: ImmutableWikicode) -> anyhow::Result<Option<ImmutableWikicode>> {
        self.replace_internal(html)
    }
}

trait Internal {
    /// `{{Template:リダイレクトの所属カテゴリ}}` の置換
    /// `to` が空の場合、テンプレートを削除する
    fn replace_internal(
        &self,
        html: ImmutableWikicode,
    ) -> anyhow::Result<Option<ImmutableWikicode>>;

    /// 1 = Category:Name1 | 2 = Category:Name2 形式の場合
    /// Category:Name1 | Category:Name2 形式の場合もindexmap上はこの形
    fn replace_internal_single(&self, template: &Template) -> anyhow::Result<bool>;

    /// 1-1 = Category:Name1 | 1-2 = Category:Name2 形式の場合
    fn replace_internal_complex(&self, template: &Template) -> anyhow::Result<bool>;
}

impl Internal for CategoryOfRedirectsReplacer {
    fn replace_internal(
        &self,
        html: ImmutableWikicode,
    ) -> anyhow::Result<Option<ImmutableWikicode>> {
        let html = html.into_mutable();

        let Ok(templates) = html.filter_templates() else {
            warn!("could not get templates");
            return Ok(None);
        };
        let templates = templates
            .into_iter()
            .filter(|template| template.name() == "Template:リダイレクトの所属カテゴリ")
            .collect::<Vec<_>>();
        if templates.is_empty() {
            return Ok(None);
        }

        let is_changed = templates
            .into_iter()
            .map(|template| {
                if template.param("redirect1").is_some() {
                    self.replace_internal_complex(&template)
                } else {
                    self.replace_internal_single(&template)
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .any(|is_changed| is_changed);

        if is_changed {
            Ok(Some(html.into_immutable()))
        } else {
            Ok(None)
        }
    }

    fn replace_internal_single(&self, template: &Template) -> anyhow::Result<bool> {
        let params = template.params();
        let mut categories = params
            .clone()
            .into_iter()
            .filter(|(k, _v)| k.parse::<u32>().is_ok())
            .map(|(_k, v)| v)
            .collect::<Vec<_>>();
        let other_properties = params
            .clone()
            .into_iter()
            .filter(|(k, _v)| k.parse::<u32>().is_err())
            .collect::<IndexMap<_, _>>();

        let Some(index) = categories.iter().position(|param| *param == self.from) else {
            return Ok(false);
        };

        categories.remove(index);
        self.to.iter().enumerate().for_each(|(i, cat)| {
            categories.insert(index + i, cat.to_string());
        });

        // 置換後、カテゴリ指定が全てなくなったらテンプレートごと削除
        if categories.is_empty() {
            template.detach();
            return Ok(true);
        }

        let mut result = IndexMap::with_capacity(other_properties.len() + categories.len());
        result.extend(other_properties);
        result.extend(
            categories
                .into_iter()
                .enumerate()
                .map(|(cat_num, cat)| ((cat_num + 1).to_string(), cat)),
        );

        if result == params {
            return Ok(false);
        }

        if let Err(err) = template.set_params(result) {
            warn!("could not set params: {:?}", err);
        }

        Ok(true)
    }

    fn replace_internal_complex(&self, template: &Template) -> anyhow::Result<bool> {
        let params = template.params();
        let redirect_key_regex = Regex::new(r"^redirect(\d|10)$")?;
        let category_key_regex = Regex::new(r"^\d-(\d|10)$")?;

        let other_properties = params
            .clone()
            .into_iter()
            .filter(|(k, _v)| !redirect_key_regex.is_match(k) && !category_key_regex.is_match(k))
            .collect::<IndexMap<_, _>>();

        let mut replaced = IndexMap::new();

        for redirect_index in 1..=10 {
            let Some(redirect) = params.get(&format!("redirect{}", redirect_index)) else {
                continue;
            };
            let mut categories = (1..=10)
                .filter_map(|cat_index| {
                    params
                        .get(&format!("{redirect_index}-{cat_index}"))
                        .cloned()
                })
                .collect::<Vec<_>>();
            if let Some(index) = categories
                .iter()
                .position(|cat| *cat == self.from.replace("Category:", ""))
            {
                categories.remove(index);
                self.to
                    .iter()
                    .for_each(|t| categories.insert(index, t.replace("Category:", "")));
                if categories.is_empty() {
                    continue;
                }
            }

            replaced.insert(format!("redirect{redirect_index}"), redirect.to_string());
            replaced.extend(categories.iter().enumerate().map(|(cat_index, b)| {
                (
                    format!("{}-{}", redirect_index, cat_index + 1),
                    b.to_string(),
                )
            }));
        }

        if replaced.is_empty() {
            template.detach();
            return Ok(true);
        }

        let mut result = IndexMap::with_capacity(other_properties.len() + replaced.len());
        result.extend(other_properties);
        result.extend(replaced);

        if result == params {
            return Ok(false);
        }

        if let Err(err) = template.set_params(result) {
            warn!("could not set params: {:?}", err);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod test {
    use frunk_core::hlist;
    use indoc::indoc;

    use super::*;
    use crate::replacer::CategoryReplacerList as _;
    use crate::util::test;

    #[tokio::test]
    async fn test_replace_redirect_category_simple() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = vec!["Category:Name2".to_string()];

        let before = indoc! {"
            {{リダイレクトの所属カテゴリ|Category:Name1}}
        "};
        let after = indoc! {"
            {{リダイレクトの所属カテゴリ|Category:Name2}}
        "};

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![CategoryOfRedirectsReplacer::new(from, to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_replace_redirect_category_simple_multiline() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = vec!["Category:Name2".to_string(), "Category:Name3".to_string()];

        let before = indoc! {"
            {{リダイレクトの所属カテゴリ
            |1 = Category:Name1
            }}
        "};
        let after = indoc! {"
            {{リダイレクトの所属カテゴリ|Category:Name2|Category:Name3}}
        "};

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![CategoryOfRedirectsReplacer::new(from, to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_redirect_category_simple() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = vec![];

        let before = indoc! {"
            {{リダイレクトの所属カテゴリ
            |1 = Category:Name1
            }}
        "};
        let after = "";

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![CategoryOfRedirectsReplacer::new(from, to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_replace_redirect_category_complex() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:アニメ作品 こ".to_string();
        let to = vec![
            "Category:アニメ作品 ほげ".to_string(),
            "Category:アニメ作品 ふが".to_string(),
        ];

        let before = indoc! {"
            {{リダイレクトの所属カテゴリ
            |redirect1 = リダイレクト1
            |1-1 = アニメ作品 こ
            |1-2 = フジテレビ系アニメ
            |redirect2 = リダイレクト2
            |2-1 = テスト
            }}
        "};
        let after = indoc! {"
            {{リダイレクトの所属カテゴリ|redirect1=リダイレクト1|1-1=アニメ作品 ふが|1-2=アニメ作品 ほげ|1-3=フジテレビ系アニメ|redirect2=リダイレクト2|2-1=テスト}}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await?
            .into_mutable();
        let template = &html.filter_templates()?[0];

        let replacer = CategoryOfRedirectsReplacer::new(from, to);
        let is_changed = replacer.replace_internal_complex(template)?;
        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_redirect_category_complex_one() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:アニメ作品 こ".to_string();
        let to = vec![];

        let before = indoc! {"
            {{リダイレクトの所属カテゴリ
            |redirect1 = リダイレクト1
            |1-1 = アニメ作品 こ
            |1-2 = アニメ作品 ほげ
            }}
        "};
        let after = indoc! {"
            {{リダイレクトの所属カテゴリ|redirect1=リダイレクト1|1-1=アニメ作品 ほげ}}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await?
            .into_mutable();
        let template = &html.filter_templates()?[0];

        let replacer = CategoryOfRedirectsReplacer::new(from, to);
        let is_changed = replacer.replace_internal_complex(template)?;
        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_redirect_category_complex_all() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:アニメ作品 こ".to_string();
        let to = vec![];

        let before = indoc! {"
            {{リダイレクトの所属カテゴリ
            |redirect1 = リダイレクト1
            |1-1 = アニメ作品 こ
            }}
        "};
        let after = "";

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await?
            .into_mutable();
        let template = &html.filter_templates()?[0];

        let replacer = CategoryOfRedirectsReplacer::new(from, to);
        let is_changed = replacer.replace_internal_complex(template)?;
        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    /// https://ja.wikipedia.org/w/index.php?diff=99117037
    #[tokio::test]
    async fn test_regression_1() -> anyhow::Result<()> {
        let bot = test::bot().await;

        let wikitext = indoc! {"\
            {{リダイレクトの所属カテゴリ
            | redirect = 東京ミュウミュウ にゅ〜♡
            | 1 = 2022年のテレビアニメ
            | 2 = 2023年のテレビアニメ
            | 3 = ゆめ太カンパニーのアニメ作品
            | 4 = グラフィニカのアニメ作品
            | 5 = ポニーキャニオンのアニメ作品
            |6=テレビ東京の深夜アニメ|7=電通のアニメ作品}}
        "};
        let html = bot.parsoid().transform_to_html(wikitext).await?;

        let replacers = hlist![CategoryOfRedirectsReplacer::new(
            "Category:ネコ".to_string(),
            vec!["Category:猫".to_string()],
        )];
        let (_replaced, is_changed) = replacers.replace_all(html).await?;

        assert!(!is_changed);

        Ok(())
    }
}
