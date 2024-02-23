use indexmap::IndexMap;
use mwbot::parsoid::prelude::*;

use crate::replacer::CategoryReplacer;

const TEMPLATES: &[&str] = &[
    "Template:画像提供依頼",
    "Template:画像募集中",
    "Template:画像改訂依頼",
];

#[derive(Debug, Clone)]
pub struct ImageRequestedReplacer {
    from: String,
    to: Vec<String>,
}

impl ImageRequestedReplacer {
    pub fn new(from: String, to: Vec<String>) -> Option<Self> {
        if !from.ends_with("の画像提供依頼") || to.iter().any(|t| !t.ends_with("の画像提供依頼"))
        {
            return None;
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

        Some(Self {
            from: from.to_string(),
            to,
        })
    }
}

impl CategoryReplacer for ImageRequestedReplacer {
    async fn replace(&self, html: ImmutableWikicode) -> anyhow::Result<Option<ImmutableWikicode>> {
        let html = html.into_mutable();

        let templates = html.filter_templates()?;
        let templates = templates
            .into_iter()
            .filter(|template| TEMPLATES.contains(&&*template.name()))
            .collect::<Vec<_>>();
        if templates.is_empty() {
            return Ok(None);
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

            let Some(index) = cats.iter().position(|c| *c == self.from) else {
                return Ok(None);
            };
            cats.remove(index);
            let already_added_cats = cats.clone();

            self.to
                .iter()
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
        Ok(Some(html.into_immutable()))
    }
}

#[cfg(test)]
mod test {
    use frunk_core::hlist;
    use indoc::indoc;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::replacer::CategoryReplacerList;
    use crate::util::test;

    #[tokio::test]
    async fn test_replace() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:伊達市 (北海道)の画像提供依頼".to_string();
        let to = vec!["Category:北海道伊達市の画像提供依頼".to_string()];

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

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![ImageRequestedReplacer::new(from, to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_add() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:伊達市 (北海道)の画像提供依頼".to_string();
        let to = vec![
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

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![ImageRequestedReplacer::new(from, to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_duplicate() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:北海道伊達市の画像提供依頼".to_string();
        let to = vec![
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

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![ImageRequestedReplacer::new(from, to).unwrap()];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }

    #[tokio::test]
    async fn test_remove() -> anyhow::Result<()> {
        let bot = test::bot().await;
        let from = "Category:伊達市 (北海道)の画像提供依頼".to_string();
        let to = vec![];

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

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![ImageRequestedReplacer::new(from, to)];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;
        assert_eq!(after, replaced_wikicode);

        Ok(())
    }
}
