use mwbot::parsoid::prelude::*;
use mwbot::Bot;

use crate::category::template::category_of_redirects;

mod template;

/// カテゴリタグ(`[[Category:Example]]`)の置換
/// `to` が空の場合、`from` のカテゴリを削除する
fn replace_category_tag(html: &Wikicode, from: impl AsRef<str>, to: impl AsRef<[String]>) {
    let (from, to) = (from.as_ref(), to.as_ref());

    let categories = html.filter_categories();
    let category_names = categories
        .iter()
        .map(|category| category.category())
        .collect::<Vec<_>>();

    let Some(category_tag) = categories
        .iter()
        .find(|category| category.category() == from)
    else {
        return;
    };

    if to.is_empty() {
        category_tag.detach();
        return;
    }

    category_tag.set_category(&to[0]);
    if to.len() == 1 {
        return;
    }

    to[1..]
        .iter()
        // 既にカテゴリタグとして追加されていたら追加しない
        .filter(|category| !category_names.contains(category))
        .for_each(|category| {
            category_tag.insert_after(&Category::new(category, None));
        });
}

pub async fn replace_category(
    bot: &Bot,
    html: &Wikicode,
    from: impl AsRef<str>,
    to: impl AsRef<[String]>,
) -> anyhow::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();

    replace_category_tag(html, from, to);
    category_of_redirects::replace(html, from, to);
    // 再帰的なテンプレートの変更
    template::replace_recursion(bot, html, from, to, &|template_param, f, t| {
        template::image_wanted::replace(template_param, f, t)
    })
    .await?;

    Ok(())
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use pretty_assertions::assert_eq;

    use super::replace_category_tag;
    use crate::util::test;

    #[tokio::test]
    async fn test_replace_redirect_tag_one() {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = "Category:Name2".to_string();

        let before = indoc! {"
            [[Category:Name1]]
        "};
        let after = indoc! {"
            [[Category:Name2]]
        "};

        let wikicode = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace_category_tag(&wikicode, &from, [to]);

        let replaced_wikicode = bot
            .parsoid()
            .transform_to_wikitext(&wikicode)
            .await
            .unwrap();

        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_replace_redirect_multiple() {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = &["Category:Name2".to_string(), "Category:Name3".to_string()];

        let before = indoc! {"
            [[Category:Name1]]
        "};
        let after = indoc! {"
            [[Category:Name2]]
            [[Category:Name3]]
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace_category_tag(&html, &from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();

        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_remove_redirect_tag() {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = &[];

        let before = indoc! {"
            [[Category:Name1]]
        "};
        let after = "";

        let wikicode = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace_category_tag(&wikicode, &from, to);

        let replaced_wikicode = bot
            .parsoid()
            .transform_to_wikitext(&wikicode)
            .await
            .unwrap();

        assert_eq!(after, replaced_wikicode);
    }
}
