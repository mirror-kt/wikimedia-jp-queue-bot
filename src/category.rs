use indexmap19::IndexMap;
use mwbot::parsoid::prelude::*;
use tracing::warn;

/// カテゴリタグ(`[[Category:Example]]`)の置換
/// `to` が空の場合、`from` のカテゴリを削除する
pub fn replace_category_tag(html: &Wikicode, from: impl AsRef<str>, to: impl AsRef<[String]>) {
    let to = to.as_ref();

    let categories = html.filter_categories();
    let category_names = categories
        .iter()
        .map(|category| category.category())
        .collect::<Vec<_>>();

    let Some(category_tag) = categories
        .iter()
        .find(|category| category.category() == *from.as_ref())
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

/// `{{Template:リダイレクトの所属カテゴリ}}` の置換
/// `to` が空の場合、テンプレートを削除する
pub fn replace_redirect_category_template(
    html: &Wikicode,
    from: impl AsRef<str>,
    to: impl AsRef<[String]>,
) {
    let from = from.as_ref();
    let to = to.as_ref();

    let Ok(templates) = html.filter_templates() else {
        warn!("could not get templates");
        return;
    };
    let templates = templates
        .into_iter()
        .filter(|template| template.name() == "Template:リダイレクトの所属カテゴリ")
        .collect::<Vec<_>>();
    if templates.is_empty() {
        return;
    }

    for template in templates {
        if template
            .params()
            .keys()
            .all(|key| key.parse::<u32>().is_ok())
        {
            replace_redirect_category_template_simple(&template, from, to);
        } else {
            replace_redirect_category_template_complex(&template, from, to);
        }
    }
}

/// 1 = Category:Name1 | 2 = Category:Name2 形式の場合
/// Category:Name1 | Category:Name2 形式の場合もindexmap上はこの形
fn replace_redirect_category_template_simple(template: &Template, from: &str, to: &[String]) {
    let mut params = template
        .params()
        .sorted_by(|k1, _, k2, _| k1.parse::<u32>().unwrap().cmp(&k2.parse::<u32>().unwrap()))
        .map(|(_k, v)| v)
        .collect::<Vec<_>>();
    let Some(index) = params.iter().position(|param| param == from) else {
        return;
    };

    params.remove(index);
    to.iter().enumerate().for_each(|(i, cat)| {
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

// 1-1 = Category:Name1 | 1-2 = Category:Name2 形式の場合
fn replace_redirect_category_template_complex(template: &Template, from: &str, to: &[String]) {
    let params = template.params();

    let mut result = IndexMap::new();

    // redirect1, redirect2... という名前のパラメータを取得
    let redirect_pages = params
        .iter()
        .filter(|(k, _)| k.starts_with("redirect"))
        .map(|(k, v)| (k, v))
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
            .position(|category| *category == from.replace("Category:", ""))
        {
            categories.remove(index);
            to.iter().enumerate().for_each(|(i, cat)| {
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

#[cfg(test)]
mod test {
    use indoc::indoc;
    use mwbot::parsoid::prelude::*;
    use pretty_assertions::assert_eq;

    use crate::test;

    use super::{
        replace_category_tag,
        replace_redirect_category_template,
        replace_redirect_category_template_complex,
    };

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

        replace_category_tag(&wikicode, &from, &[to]);

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

    #[tokio::test]
    async fn test_replace_redirect_category_simple() {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = "Category:Name2".to_string();

        let before = indoc! {"
            {{リダイレクトの所属カテゴリ|Category:Name1}}
        "};
        let after = indoc! {"
            {{リダイレクトの所属カテゴリ|Category:Name2}}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace_redirect_category_template(&html, &from, &[to]);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_replace_redirect_category_simple_multiline() {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = &["Category:Name2".to_string(), "Category:Name3".to_string()];

        let before = indoc! {"
            {{リダイレクトの所属カテゴリ
            |1 = Category:Name1
            }}
        "};
        let after = indoc! {"
            {{リダイレクトの所属カテゴリ|Category:Name2|Category:Name3}}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace_redirect_category_template(&html, &from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_remove_redirect_category_simple() {
        let bot = test::bot().await;
        let from = "Category:Name1".to_string();
        let to = &[];

        let before = indoc! {"
            {{リダイレクトの所属カテゴリ
            |1 = Category:Name1
            }}
        "};
        let after = "";

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace_redirect_category_template(&html, &from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_replace_redirect_category_complex() {
        let bot = test::bot().await;
        let from = "Category:アニメ作品 こ";
        let to = &[
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
            .await
            .unwrap()
            .into_mutable();
        let template = &html.filter_templates().unwrap()[0];

        replace_redirect_category_template_complex(template, from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_remove_redirect_category_complex_one() {
        let bot = test::bot().await;
        let from = "Category:アニメ作品 こ";
        let to = &[];

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
            .await
            .unwrap()
            .into_mutable();
        let template = &html.filter_templates().unwrap()[0];

        replace_redirect_category_template_complex(template, from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_remove_redirect_category_complex_all() {
        let bot = test::bot().await;
        let from = "Category:アニメ作品 こ";
        let to = &[];

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
            .await
            .unwrap()
            .into_mutable();
        let template = &html.filter_templates().unwrap()[0];

        replace_redirect_category_template_complex(template, from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }
}
