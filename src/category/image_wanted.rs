use indexmap19::IndexMap;
use mwbot::parsoid::prelude::*;
use tracing::warn;

const TEMPLATES: &[&str] = &[
    "Template:画像提供依頼",
    "Template:画像募集中",
    "Template:画像改訂依頼",
];

pub fn replace(html: &Wikicode, from: impl AsRef<str>, to: impl AsRef<[String]>) {
    let from = from.as_ref();
    let to = to.as_ref();

    if !from.ends_with("の画像提供依頼") || to.iter().any(|t| !t.ends_with("の画像提供依頼"))
    {
        return;
    }
    let from = from
        .trim_start_matches("Category:")
        .trim_end_matches("の画像提供依頼");
    let to = to
        .iter()
        .map(|t| {
            t.trim_start_matches("Category:")
                .trim_end_matches("の画像提供依頼")
        })
        .collect::<Vec<_>>();

    let Ok(templates) = html.filter_templates() else {
        warn!("could not get templates");
        return;
    };
    let templates = templates
        .into_iter()
        .filter(|template| TEMPLATES.contains(&&*template.name()))
        .collect::<Vec<_>>();
    if templates.is_empty() {
        return;
    }

    for template in templates {
        let params = template.params();
        let mut cats = params
            .iter()
            .filter_map(|(k, v)| {
                if k.starts_with("cat") {
                    Some(v.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let Some(index) = cats.iter().position(|c| c == from) else {
            return;
        };
        cats.remove(index);
        let already_added_cats = cats.clone();

        to.iter()
            .filter(|c| !already_added_cats.contains(&c.to_string()))
            .enumerate()
            .for_each(|(i, cat)| {
                cats.insert(index + i, cat.to_string());
            });

        let mut params = params
            .iter()
            .filter(|(k, _)| !k.starts_with("cat"))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<IndexMap<String, String>>();

        for (i, cat) in cats.iter().enumerate() {
            let key = if i == 0 {
                "cat".to_string()
            } else {
                format!("cat{}", i + 1)
            };
            params.insert(key, cat.to_string());
        }
        if let Err(err) = template.set_params(params) {
            warn!("could not set params: {}", err)
        }
    }
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use mwbot::parsoid::WikinodeIterator;

    use crate::util::test;

    #[tokio::test]
    async fn test_replace() {
        let bot = test::bot().await;
        let from = "Category:伊達市 (北海道)の画像提供依頼";
        let to = &["Category:北海道伊達市の画像提供依頼".to_string()];

        let before = indoc! {"
            {{画像提供依頼
            |各施設外観
            |date=2017年7月
            |cat=伊達市 (北海道)
            }}
        "};
        let after = indoc! {"
            {{画像提供依頼|各施設外観|date=2017年7月|cat=北海道伊達市}}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace(&html, from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_add() {
        let bot = test::bot().await;
        let from = "Category:伊達市 (北海道)の画像提供依頼";
        let to = &[
            "Category:北海道伊達市の画像提供依頼".to_string(),
            "Category:北海道の画像提供依頼".to_string(),
        ];

        let before = indoc! {"
            {{画像提供依頼
            |各施設外観
            |date=2017年7月
            |cat=伊達市 (北海道)
            }}
        "};
        let after = indoc! {"
            {{画像提供依頼|各施設外観|date=2017年7月|cat=北海道伊達市|cat2=北海道}}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace(&html, from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_duplicate() {
        let bot = test::bot().await;
        let from = "Category:北海道伊達市の画像提供依頼";
        let to = &[
            "Category:北海道伊達市の画像提供依頼".to_string(),
            "Category:北海道の画像提供依頼".to_string(),
        ];

        let before = indoc! {"
            {{画像提供依頼
            |各施設外観
            |date=2017年7月
            |cat=北海道伊達市
            }}
        "};
        let after = indoc! {"
            {{画像提供依頼|各施設外観|date=2017年7月|cat=北海道伊達市|cat2=北海道}}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace(&html, from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_remove() {
        let bot = test::bot().await;
        let from = "Category:伊達市 (北海道)の画像提供依頼";
        let to: &[String] = &[];

        let before = indoc! {"
            {{画像提供依頼
            |各施設外観
            |date=2017年7月
            |cat=伊達市 (北海道)
            }}
        "};
        let after = indoc! {"
            {{画像提供依頼|各施設外観|date=2017年7月}}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace(&html, from, to);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test() {
        let bot = test::bot().await;
        let page = bot.page("伊達赤十字看護専門学校").unwrap();
        let html = page.html().await.unwrap().into_mutable();

        let templates = html.filter_templates().unwrap();

        dbg!(templates
            .iter()
            .map(|t| (t.name(), t.params()))
            .collect::<Vec<_>>());
    }
}
