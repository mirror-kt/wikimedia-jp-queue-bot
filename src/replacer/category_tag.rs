use mwbot::parsoid::prelude::*;

use crate::replacer::CategoryReplacer;

/// カテゴリタグ(`[[Category:Example]]`)の置換
/// `to` が空の場合、`from` のカテゴリを削除する
#[derive(Debug, Clone)]
pub struct CategoryTagReplacer {
    from: String,
    to: Vec<String>,
}

impl CategoryTagReplacer {
    pub fn new(from: String, to: Vec<String>) -> Self {
        Self { from, to }
    }
}

impl CategoryReplacer for CategoryTagReplacer {
    async fn replace(&self, html: ImmutableWikicode) -> anyhow::Result<Option<ImmutableWikicode>> {
        let html = html.into_mutable();
        let mut categories = html.filter_categories();

        if self.to.contains(&self.from)
            && self
                .to
                .iter()
                .all(|to| categories.iter().any(|cat| cat.category() == *to))
        {
            return Ok(None);
        }

        let Some(index) = categories
            .iter()
            .position(|category| category.category() == self.from)
        else {
            return Ok(None);
        };
        let from = categories.remove(index);

        self.to
            .iter()
            .filter(|to| !categories.iter().any(|cat| cat.category() == **to))
            .for_each(|cat| {
                dbg!(&cat);
                if *cat == from.category() {
                    from.insert_before(&Category::new(cat, from.sort_key().as_deref()));
                } else {
                    from.insert_before(&Category::new(cat, None));
                }
            });
        from.detach();
        dbg!(html
            .filter_categories()
            .iter()
            .map(|cat| cat.category())
            .collect::<Vec<_>>());

        Ok(Some(html.into_immutable()))
    }
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use pretty_assertions::assert_str_eq;
    use rstest::rstest;

    use crate::replacer::category_tag::CategoryTagReplacer;
    use crate::replacer::CategoryReplacer;
    use crate::util::test;

    #[rstest(::trace)]
    // Simple reassignment categories
    #[case(
        "Category:Name1",
        &["Category:Name2"],
        indoc ! {"\
            [[Category:Name1]]
        "},
        indoc ! {"\
            [[Category:Name2]]
        "},
        true,
    )]
    #[case(
        "Category:Name1",
        &["Category:Name2", "Category:Name3"],
        indoc! {"\
            [[Category:Name1]]
        "},
        indoc! {"\
            [[Category:Name2]]
            [[Category:Name3]]
        "},
        true,
    )]
    // no duplicate tag when category is already added
    #[case(
        "Category:Name1",
        &["Category:Name2"],
        indoc!{"\
            [[Category:Name1]]
            [[Category:Name2]]
        "},
        indoc!{"
            
            [[Category:Name2]]
        "},
        true,
    )]
    // Remove categories
    #[case(
        "Category:Name1",
        &[],
        indoc!{"\
            [[Category:Name1]]
        "},
        "",
        true,
    )]
    // Duplicate categories
    #[case(
        "Category:東京都の区立図書館",
        &["Category:日本の公共図書館", "Category:東京都の区立図書館"],
        indoc!{"\
            [[Category:東京都の区立図書館]]
        "},
        indoc!{"\
            [[Category:日本の公共図書館]]
            [[Category:東京都の区立図書館]]
        "},
        true,
    )]
    // Regression test for https://ja.wikipedia.org/w/index.php?oldid=99373661#複製キューの誤動作
    #[case(
        "Category:福井県の市町村立図書館",
        &["Category:日本の公共図書館", "Category:福井県の市町村立図書館"],
        indoc!{"\
            [[Category:日本の公共図書館|廃ふくいしりつふくい]]
            [[Category:日本の市町村立図書館 (廃止)]]
            [[Category:福井県の市町村立図書館|廃ふくいしりつふくい]]
        "},
        indoc!{"\
            [[Category:日本の公共図書館|廃ふくいしりつふくい]]
            [[Category:日本の市町村立図書館 (廃止)]]
            [[Category:福井県の市町村立図書館|廃ふくいしりつふくい]]
        "},
        false,
    )]
    #[tokio::test]
    async fn test_replace(
        #[case] from: &str,
        #[case] to: &[&str],
        #[case] before_wikitext: &str,
        #[case] after_wikitext: &str,
        #[case] should_be_changed: bool,
    ) -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = from.to_string();
        let to = to.iter().map(|x| x.to_string()).collect::<Vec<_>>();

        let html = bot.parsoid().transform_to_html(before_wikitext).await?;

        let replacer = CategoryTagReplacer::new(from.to_string(), to);
        let replaced = replacer.replace(html).await?;

        if should_be_changed {
            let replaced_wikitext = bot
                .parsoid()
                .transform_to_wikitext(&replaced.expect("wikitext should be changed"))
                .await?;
            dbg!(&replaced_wikitext);
            assert_str_eq!(after_wikitext, replaced_wikitext);
        } else {
            assert!(replaced.is_none());
        }

        Ok(())
    }
}
