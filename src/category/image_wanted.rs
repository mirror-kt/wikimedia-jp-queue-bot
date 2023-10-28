use async_recursion::async_recursion;
use indexmap19::IndexMap;
use mwbot::parsoid::prelude::*;
use mwbot::Bot;

use crate::util::JoinAllExt as _;

const TEMPLATES: &[&str] = &[
    "Template:画像提供依頼",
    "Template:画像募集中",
    "Template:画像改訂依頼",
];

pub async fn replace(
    bot: &Bot,
    html: &Wikicode,
    from: impl AsRef<str>,
    to: impl AsRef<[String]>,
) -> anyhow::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();

    if !from.ends_with("の画像提供依頼") || to.iter().any(|t| !t.ends_with("の画像提供依頼"))
    {
        return Ok(());
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

    replace_recursion(bot, html, from, &to).await.map(|_| ())
}

/// Ok(true): `html` の引数を書き換えたとき
#[async_recursion(?Send)]
async fn replace_recursion(
    bot: &Bot,
    html: &Wikicode,
    from: &str,
    to: &[&str],
) -> anyhow::Result<bool> {
    let mut is_changed = replace_internal(html, from, to)?;

    let templates = html.filter_templates()?;
    if templates.is_empty() {
        return Ok(is_changed);
    }

    for template in templates {
        let params = template.params();

        let new_params = params
            .into_iter()
            .map(|(k, v)| {
                let bot = bot.clone();
                tokio::spawn(async move { (bot.parsoid().transform_to_html(&v).await, k, v) })
            })
            .join_all()
            .await
            .into_iter()
            .flat_map(|res| {
                let (parsed_v, k, v) = res.unwrap(); // tokio panic handle
                parsed_v
                    .map(|parsed_v| (parsed_v, k.clone(), v.clone()))
                    .map_err(|_err| (k, v))
            })
            .map(|(parsed_v, k, v)| {
                let parsed_v = parsed_v.clone().into_mutable();
                async move {
                    match replace_recursion(bot, &parsed_v, from, to).await {
                        Ok(true) => Ok((parsed_v.into_immutable(), k, v)),
                        _ => Err((k, v)),
                    }
                }
            })
            .join_all()
            .await
            .into_iter()
            .map(|res| {
                let bot = bot.clone();
                tokio::spawn(async move {
                    match res {
                        Ok((new_v, k, v)) => Ok((
                            bot.parsoid().transform_to_wikitext(&new_v.clone()).await,
                            k,
                            v,
                        )),
                        Err((k, v)) => Err((k, v)),
                    }
                })
            })
            .join_all()
            .await
            .into_iter()
            .map(|res| {
                res.unwrap() // tokio panic handle
            })
            .map(|res| match res {
                Ok((new_v, k, v)) => {
                    is_changed = true;
                    (k, new_v.unwrap_or(v))
                }
                Err((k, v)) => (k, v),
            })
            .collect::<IndexMap<String, String>>();

        let _ = template.set_params(new_params);
    }

    Ok(is_changed)
}

/// Ok(true): テンプレートを書き換えたとき
/// Ok(false): テンプレートを書き換えなかったとき
fn replace_internal(html: &Wikicode, from: &str, to: &[&str]) -> anyhow::Result<bool> {
    let templates = html.filter_templates()?;
    let templates = templates
        .into_iter()
        .filter(|template| TEMPLATES.contains(&&*template.name()))
        .collect::<Vec<_>>();
    if templates.is_empty() {
        return Ok(false);
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
            return Ok(false);
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
        template.set_params(params)?;
    }
    Ok(true)
}

#[cfg(test)]
mod test {
    use indoc::indoc;

    use super::replace;
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

        replace(&bot, &html, from, to).await.unwrap();

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

        replace(&bot, &html, from, to).await.unwrap();

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

        replace(&bot, &html, from, to).await.unwrap();

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

        replace(&bot, &html, from, to).await.unwrap();

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }

    #[tokio::test]
    async fn test_nested() {
        let bot = test::bot().await;
        let from = "Category:伊達市 (北海道)の画像提供依頼";
        let to = &["Category:北海道伊達市の画像提供依頼".to_string()];

        let before = indoc! {"
            {{専修学校
            | 国 = 日本
            | 学校名 = 伊達赤十字看護専門学校
            | ふりがな = だてせきじゅうじかんごせんもんがっこう
            | 英称 = The Japanese Red Cross <br />Date School of Nursing
            | 学校の略称 =
            | 画像 = {{画像募集中|cat=伊達市 (北海道)}}
            | 画像説明 =
            | 学校設置年 =
            | 創立年 = [[1944年]]（[[昭和]]19年）4月
            | 学校種別 = 私立
            | 設置者 = 日本赤十字社 社長<br />代理 日本赤十字社 北海道支部長（[[北海道知事]]）
            | 郵便番号 = 052-0021
            | 本部所在地 = 北海道伊達市末永町81-12
            | 緯度度 =
            | 経度度 =
            | 学科 = [[看護学科]] 3年制
            | ウェブサイト = [http://www6.ocn.ne.jp/~datekan/ 公式サイト]
            }}
        "};
        let after = indoc! {"
            {{専修学校\
            |国=日本\
            |学校名=伊達赤十字看護専門学校\
            |ふりがな=だてせきじゅうじかんごせんもんがっこう\
            |英称=The Japanese Red Cross <br />Date School of Nursing\
            |学校の略称=\
            |画像={{画像募集中|cat=北海道伊達市}}\
            |画像説明=\
            |学校設置年=\
            |創立年=[[1944年]]（[[昭和]]19年）4月\
            |学校種別=私立\
            |設置者=日本赤十字社 社長<br />代理 日本赤十字社 北海道支部長（[[北海道知事]]）\
            |郵便番号=052-0021\
            |本部所在地=北海道伊達市末永町81-12\
            |緯度度=\
            |経度度=\
            |学科=[[看護学科]] 3年制\
            |ウェブサイト=[http://www6.ocn.ne.jp/~datekan/ 公式サイト]\
            }}
        "};

        let html = bot
            .parsoid()
            .transform_to_html(before)
            .await
            .unwrap()
            .into_mutable();

        replace(&bot, &html, from, to).await.unwrap();

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }
}
