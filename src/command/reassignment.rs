use std::vec;

use indexmap19::IndexMap;
use mwbot::generators::{Generator as _, Search};
use mwbot::parsoid::prelude::*;
use mwbot::{Bot, SaveOptions};
use tracing::warn;

use crate::action::move_page;
use crate::is_emergency_stopped;

use super::Status;

#[tracing::instrument]
pub async fn reassignment(
    bot: &Bot,
    from: &String,
    to: &[String],
    discussion_link: &String,
    include_article: bool,
    include_category: bool,
) -> anyhow::Result<Status> {
    let to_page = bot.page(&to[0])?;
    if !to_page.exists().await? {
        move_page(bot, &from, &to[0], format!("BOT: {}", &discussion_link)).await?;
    }

    let mut search = Search::new(format!(r#"insource:"{}""#, &from))
        .namespace(vec![
            0,  // 標準名前空間
            14, // Category名前空間
        ])
        .generate(bot);

    let mut done_count = 0;
    while let Some(page) = search.recv().await {
        if is_emergency_stopped(bot).await {
            return Ok(Status::EmergencyStopped);
        }

        let Ok(page) = page else {
            warn!("Error while searching: {:?}", page);
            continue;
        };
        if page.is_category() && !include_category {
            continue;
        }
        if !page.is_category() && !include_article {
            continue;
        }

        let Ok(html) = page.html().await.map(|html| html.into_mutable()) else {
            warn!("Error while getting html: {:?}", page);
            continue;
        };

        replace_category_tag(&html, from, to);

        let _ = page
            .save(
                html,
                &SaveOptions::summary(&format!("BOT: カテゴリの変更 ({})", &discussion_link)),
            )
            .await;
        done_count += 1;
    }

    Ok(Status::Done { done_count })
}

/// カテゴリタグ(`[[Category:Example]]`)の置換
fn replace_category_tag(html: &Wikicode, from: &String, to: &[String]) {
    let categories = html.filter_categories();
    let category_names = categories
        .iter()
        .map(|category| category.category())
        .collect::<Vec<_>>();

    let Some(category_tag) = categories
        .iter()
        .find(|category| category.category() == *from)
    else {
        return;
    };

    category_tag.set_category(&to[0]);
    category_tag.set_sort_key(None);
    if to.len() > 1 {
        to[1..]
            .iter()
            // 既にカテゴリタグとして追加されていたら追加しない
            .filter(|category| !category_names.contains(category))
            .for_each(|category| {
                category_tag.insert_after(&Category::new(category, None));
            });
    }
}

/// `{{Template:リダイレクトの所属カテゴリ}}` の置換
fn replace_redirect_category_template(html: &Wikicode, from: &str, to: &[String]) {
    let Ok(templates) = html.filter_templates() else {
        warn!("could not get templates");
        return;
    };
    let redirect_category_templates = templates
        .iter()
        .filter(|template| template.name() == "Template:リダイレクトの所属カテゴリ")
        .collect::<Vec<_>>();
    if redirect_category_templates.is_empty() {
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
    if let Some(collapse) = params.get("collapse") {
        result.insert("collapse".to_string(), collapse.to_string());
    }
    if let Some(header) = params.get("header") {
        result.insert("header".to_string(), header.to_string());
    }

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
        result.insert(redirect_str.to_string(), redirect_page.to_string());
        categories.iter().enumerate().for_each(|(i, cat)| {
            result.insert(
                format!("{}{}", category_number_prefix, i + 1),
                cat.replace("Category:", ""),
            );
        });
    }

    if let Err(err) = template.set_params(result) {
        warn!("could not set params: {}", err);
    }
}

#[cfg(test)]
mod test {
    use crate::test;
    use indexmap19::indexmap;
    use indoc::indoc;
    use mwbot::parsoid::prelude::*;

    use super::{
        replace_category_tag, replace_redirect_category_template,
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
    async fn test_replace_redirect_category_complex() {
        let template = Template::new(
            "リダイレクトの所属カテゴリ",
            &indexmap! {
                "redirect1".to_string() => "リダイレクト1".to_string(),
                "1-1".to_string() => "アニメ作品 こ".to_string(),
                "1-2".to_string() => "フジテレビ系アニメ".to_string(),
                "redirect2".to_string() => "リダイレクト2".to_string(),
                "2-1".to_string() => "テスト".to_string(),
            },
        )
        .unwrap();

        replace_redirect_category_template_complex(
            &template,
            "Category:アニメ作品 こ",
            &[
                "Category:アニメ作品 ほげ".to_string(),
                "Category:アニメ作品 ふが".to_string(),
            ],
        );

        assert_eq!(
            indexmap! {
                "redirect1".to_string() => "リダイレクト1".to_string(),
                "1-1".to_string() => "アニメ作品 ほげ".to_string(),
                "1-2".to_string() => "アニメ作品 ふが".to_string(),
                "1-3".to_string() => "フジテレビ系アニメ".to_string(),
                "redirect2".to_string() => "リダイレクト2".to_string(),
                "2-1".to_string() => "テスト".to_string(),
            },
            template.params(),
        );
    }
}
