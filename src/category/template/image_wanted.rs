use indexmap19::IndexMap;
use mwbot::parsoid::prelude::*;

const TEMPLATES: &[&str] = &[
    "Template:画像提供依頼",
    "Template:画像募集中",
    "Template:画像改訂依頼",
];

/// Ok(true): テンプレートを書き換えたとき
/// Ok(false): テンプレートを書き換えなかったとき
pub fn replace(html: &Wikicode, from: &str, to: &[String]) -> anyhow::Result<bool> {
    if !from.ends_with("の画像提供依頼") || to.iter().any(|t| !t.ends_with("の画像提供依頼"))
    {
        return Ok(false);
    }
    let from = from
        .trim_start_matches("Category:")
        .trim_end_matches("の画像提供依頼");
    let to = to
        .iter()
        .map(|t| {
            t.trim_start_matches("Category:")
                .trim_end_matches("の画像提供依頼")
                .to_string()
        })
        .collect::<Vec<_>>();

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
    use pretty_assertions::assert_eq;

    use super::replace;
    use crate::category::template;
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

        replace(&html, from, to).unwrap();

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

        replace(&html, from, to).unwrap();

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

        replace(&html, from, to).unwrap();

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

        replace(&html, from, to).unwrap();

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

        template::replace_recursion(&bot, &html, from, to, &replace)
            .await
            .unwrap();

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&html).await.unwrap();
        assert_eq!(after, replaced_wikicode);
    }
}
