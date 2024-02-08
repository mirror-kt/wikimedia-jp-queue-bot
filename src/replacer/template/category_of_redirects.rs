use async_trait::async_trait;
use indexmap::IndexMap;
use mwbot::parsoid::prelude::*;
use tracing::warn;

use crate::replacer::CategoryReplacer;

#[derive(Debug)]
pub struct CategoryOfRedirectsReplacer {
    from: String,
    to: Vec<String>,
}

impl CategoryOfRedirectsReplacer {
    pub fn new(from: String, to: Vec<String>) -> Self {
        Self { from, to }
    }
}

#[async_trait]
impl CategoryReplacer for CategoryOfRedirectsReplacer {
    async fn replace(&self, html: ImmutableWikicode) -> anyhow::Result<Option<ImmutableWikicode>> {
        Ok(self.replace_internal(html))
    }
}

trait Internal {
    /// `{{Template:リダイレクトの所属カテゴリ}}` の置換
    /// `to` が空の場合、テンプレートを削除する
    fn replace_internal(&self, html: ImmutableWikicode) -> Option<ImmutableWikicode>;

    /// 1 = Category:Name1 | 2 = Category:Name2 形式の場合
    /// Category:Name1 | Category:Name2 形式の場合もindexmap上はこの形
    fn replace_internal_single(&self, template: &Template);

    /// 1-1 = Category:Name1 | 1-2 = Category:Name2 形式の場合
    fn replace_internal_complex(&self, template: &Template);
}

impl Internal for CategoryOfRedirectsReplacer {
    fn replace_internal(&self, html: ImmutableWikicode) -> Option<ImmutableWikicode> {
        let html = html.into_mutable();

        let Ok(templates) = html.filter_templates() else {
            warn!("could not get templates");
            return None;
        };
        let templates = templates
            .into_iter()
            .filter(|template| template.name() == "Template:リダイレクトの所属カテゴリ")
            .collect::<Vec<_>>();
        if templates.is_empty() {
            return None;
        }

        for template in templates {
            if template
                .params()
                .keys()
                .all(|key| key.parse::<u32>().is_ok())
            {
                self.replace_internal_single(&template);
            } else {
                self.replace_internal_complex(&template);
            }
        }

        Some(html.into_immutable())
    }

    fn replace_internal_single(&self, template: &Template) {
        let mut params = template
            .params()
            .sorted_by(|k1, _, k2, _| k1.parse::<u32>().unwrap().cmp(&k2.parse::<u32>().unwrap()))
            .map(|(_k, v)| v)
            .collect::<Vec<_>>();
        let Some(index) = params.iter().position(|param| *param == self.from) else {
            return;
        };

        params.remove(index);
        self.to.iter().enumerate().for_each(|(i, cat)| {
            params.insert(index + i, cat.to_string());
        });

        // 置換後、カテゴリ指定が全てなくなったらテンプレートごと削除
        if params.is_empty() {
            template.detach();
            return;
        }

        let indexmap = params
            .iter()
            .enumerate()
            .map(|(i, v)| ((i + 1).to_string(), v.to_string()))
            .collect::<IndexMap<String, String>>();

        if let Err(err) = template.set_params(indexmap) {
            warn!("could not set params: {}", err);
        }
    }

    fn replace_internal_complex(&self, template: &Template) {
        let params = template.params();

        let mut result = IndexMap::new();

        // redirect1, redirect2... という名前のパラメータを取得
        let redirect_pages = params
            .iter()
            .filter(|(k, _)| k.starts_with("redirect"))
            .collect::<Vec<_>>();

        for (redirect_str, redirect_page) in redirect_pages {
            let redirect_number = redirect_str
                .chars()
                .skip_while(|c| !c.is_numeric())
                .collect::<String>();

            let category_number_prefix = format!("{}-", redirect_number);

            let mut categories = params
                .clone()
                .sorted_by(|k1, _v1, k2, _v2| k1.cmp(k2))
                .filter(|(k, _v)| k.starts_with(&category_number_prefix))
                .map(|(_k, v)| v) // SAFETY(unwrap): checked
                .collect::<Vec<_>>();

            if let Some(index) = categories
                .iter()
                .position(|category| *category == self.from.replace("Category:", ""))
            {
                categories.remove(index);
                self.to.iter().enumerate().for_each(|(i, cat)| {
                    categories.insert(index + i, cat.to_string());
                });
            }

            // set results
            // カテゴリ指定が空ならリダイレクトページ指定ごと削除
            if !categories.is_empty() {
                result.insert(redirect_str.to_string(), redirect_page.to_string());
                categories.iter().enumerate().for_each(|(i, cat)| {
                    result.insert(
                        format!("{}{}", category_number_prefix, i + 1),
                        cat.replace("Category:", ""),
                    );
                });
            }
        }

        // 置換後、カテゴリ指定が全てなくなったらテンプレートごと削除
        if result.is_empty() {
            template.detach();
            return;
        }

        let mut new_param = IndexMap::new();

        if let Some(collapse) = params.get("collapse") {
            new_param.insert("collapse".to_string(), collapse.to_string());
        }
        if let Some(header) = params.get("header") {
            new_param.insert("header".to_string(), header.to_string());
        }
        new_param.extend(result);

        if let Err(err) = template.set_params(new_param) {
            warn!("could not set params: {}", err);
        }
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
            {{リダイレクトの所属カテゴリ|redirect1=リダイレクト1|1-1=アニメ作品 ほげ|1-2=アニメ作品 ふが|1-3=フジテレビ系アニメ|redirect2=リダイレクト2|2-1=テスト}}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await?
            .into_mutable();
        let template = &html.filter_templates()?[0];

        let replacer = CategoryOfRedirectsReplacer::new(from, to);
        replacer.replace_internal_complex(template);

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
        replacer.replace_internal_complex(template);

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
        replacer.replace_internal_complex(template);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }
}
