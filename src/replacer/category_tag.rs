use async_trait::async_trait;
use mwbot::parsoid::prelude::*;

use crate::replacer::CategoryReplacer;

/// カテゴリタグ(`[[Category:Example]]`)の置換
/// `to` が空の場合、`from` のカテゴリを削除する
#[derive(Debug)]
pub struct CategoryTagReplacer {
    from: String,
    to: Vec<String>,
}

impl CategoryTagReplacer {
    pub fn new(from: String, to: Vec<String>) -> Self {
        Self { from, to }
    }
}

#[async_trait]
impl CategoryReplacer for CategoryTagReplacer {
    async fn replace(&self, html: ImmutableWikicode) -> anyhow::Result<Option<ImmutableWikicode>> {
        let html = html.into_mutable();
        let categories = html.filter_categories();

        let Some(category_tag) = categories
            .iter()
            .find(|category| category.category() == self.from)
        else {
            return Ok(None);
        };

        if self.to.is_empty() {
            category_tag.detach();
            return Ok(Some(html.into_immutable()));
        }

        category_tag.set_category(&self.to[0]);
        if self.to.len() == 1 {
            return Ok(Some(html.into_immutable()));
        }

        self.to[1..].iter().for_each(|category| {
            category_tag.insert_after(&Category::new(category, None));
        });

        Ok(Some(html.into_immutable()))
    }
}

#[cfg(test)]
mod test {
    use frunk_core::hlist;
    use indoc::indoc;
    use pretty_assertions::assert_eq;

    use crate::replacer::category_tag::CategoryTagReplacer;
    use crate::replacer::CategoryReplacerList;
    use crate::util::test;

    #[tokio::test]
    async fn test_replace_category_tag_one() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = vec!["Category:Name2".to_string()];

        let before = indoc! {"
            [[Category:Name1]]
        "};
        let after = indoc! {"
            [[Category:Name2]]
        "};

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![CategoryTagReplacer::new(from.to_string(), to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;

        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_replace_category_tag_multiple() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = vec!["Category:Name2".to_string(), "Category:Name3".to_string()];

        let before = indoc! {"
            [[Category:Name1]]
        "};
        let after = indoc! {"
            [[Category:Name2]]
            [[Category:Name3]]
        "};

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![CategoryTagReplacer::new(from, to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;

        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_category_tag() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = vec![];

        let before = indoc! {"
            [[Category:Name1]]
        "};
        let after = "";

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![CategoryTagReplacer::new(from, to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;

        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_duplicate_tag() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:東京都の区立図書館".to_string();
        let to = vec![
            "Category:日本の公共図書館".to_string(),
            "Category:東京都の区立図書館".to_string(),
        ];

        let before = indoc! {"\
            [[Category:東京都の区立図書館]]
        "};
        let after = indoc! {"\
            [[Category:日本の公共図書館]]
            [[Category:東京都の区立図書館]]
        "};

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![CategoryTagReplacer::new(from, to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }
}
