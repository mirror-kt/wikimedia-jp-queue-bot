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
        let mut categories = html.filter_categories();

        let Some(index) = categories
            .iter()
            .position(|cat| cat.category() == self.from)
        else {
            return Ok(None);
        };
        let from = categories.remove(index);

        let mut changed = 0;
        self.to
            .iter()
            .filter(|to| categories.iter().all(|cat| cat.category() != **to))
            .map(|to| {
                if self.from == *to {
                    Category::new(to, from.sort_key().as_deref())
                } else {
                    Category::new(to, None)
                }
            })
            .for_each(|to| {
                from.insert_before(&to);
                changed += 1;
            });
        from.detach();

        if !self.to.contains(&self.from) && changed > 0 {
            // 再配属の場合 追加したカテゴリが1つ以上の場合は変更がある
            Ok(Some(html.into_immutable()))
        } else if self.to.contains(&self.from) && changed > 1 {
            // 複製の場合 複製元のカテゴリも1度除去されて再追加されるので、変更がない場合もカウントが1になる
            Ok(Some(html.into_immutable()))
        } else if self.to.is_empty() && changed == 0 {
            // 除去の場合
            Ok(Some(html.into_immutable()))
        } else {
            // 変更がない場合
            Ok(None)
        }
    }
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use crate::replacer::category_tag::CategoryTagReplacer;
    use crate::replacer::CategoryReplacer;
    use crate::util::test;

    #[rstest]
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
    // noop when category is already added
    #[case(
        "Category:Name1",
        &["Category:Name2"],
        indoc!{"\
            [[Category:Name1]]
            [[Category:Name2]]
        "},
        indoc!{"\
            [[Category:Name1]]
            [[Category:Name2]]
        "},
        false,
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
        #[case] should_changed: bool,
    ) -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = from.to_string();
        let to = to.iter().map(|x| x.to_string()).collect::<Vec<_>>();

        let html = bot.parsoid().transform_to_html(before_wikitext).await?;

        let replacer = CategoryTagReplacer::new(from.to_string(), to);
        let replaced = replacer.replace(html).await?;

        if should_changed {
            let replaced_html = replaced.expect("wikitext should be changed");
            let replaced_wikitext = bot.parsoid().transform_to_wikitext(&replaced_html).await?;
            assert_eq!(after_wikitext, replaced_wikitext);
        } else {
            assert!(replaced.is_none());
        }

        Ok(())
    }
}
