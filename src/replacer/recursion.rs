use async_trait::async_trait;
use futures_util::{stream, Stream, StreamExt, TryStreamExt};
use indexmap::IndexMap;
use mwbot::parsoid::prelude::*;
use mwbot::Bot;

use crate::replacer::{CategoryReplacer, CategoryReplacerList};

#[derive(Debug)]
pub struct RecursionReplacer<ReplacerList> {
    bot: Bot,
    replacers: ReplacerList,
}

impl<ReplacerList> RecursionReplacer<ReplacerList>
where
    ReplacerList: CategoryReplacerList,
{
    pub fn new(bot: Bot, replacers: ReplacerList) -> Self {
        Self { bot, replacers }
    }
}

#[async_trait]
impl<ReplacerList> CategoryReplacer for RecursionReplacer<ReplacerList>
where
    ReplacerList: CategoryReplacerList,
{
    async fn replace(&self, html: ImmutableWikicode) -> anyhow::Result<Option<ImmutableWikicode>> {
        let (replaced, mut is_changed) = self.replacers.replace_all(html).await?;

        let templates = replaced
            .clone()
            .into_mutable()
            .filter_templates()?
            .into_iter()
            .enumerate()
            .map(|(index, template)| (index, template.params()))
            .collect::<Vec<_>>();

        let replaced_templates = stream::iter(templates)
            .then(|(index, params)| async move {
                let replaced_params = stream::iter(params.clone())
                    .params_to_html(&self.bot)
                    .replace_params(self)
                    .params_to_wikitext(&self.bot)
                    .try_collect::<Vec<_>>()
                    .await?;

                Ok::<_, anyhow::Error>((index, replaced_params))
            })
            .try_collect::<Vec<_>>()
            .await?;

        let html = replaced.into_mutable();
        let templates = html.filter_templates()?;

        for (index, params) in replaced_templates {
            let template = &templates[index];
            let old_params = template.params();

            if params.iter().all(|(_k, v)| v.is_none()) {
                continue;
            }

            let new_params = params
                .into_iter()
                .map(|(k, v)| (k.clone(), v.unwrap_or_else(|| old_params[&k].clone())))
                .collect::<IndexMap<_, _>>();

            let _ = templates[index].set_params(new_params);
            is_changed = true;
        }

        if is_changed {
            Ok(Some(html.into_immutable()))
        } else {
            Ok(None)
        }
    }
}

trait CustomStreamExt {
    fn params_to_html(
        self,
        bot: &Bot,
    ) -> impl Stream<Item = anyhow::Result<(String, ImmutableWikicode)>>
    where
        Self: Stream<Item = (String, String)>;

    fn replace_params<'r, ReplacerList>(
        self,
        replacer: &'r RecursionReplacer<ReplacerList>,
    ) -> impl Stream<Item = anyhow::Result<(String, Option<ImmutableWikicode>)>> + 'r
    where
        Self: Stream<Item = anyhow::Result<(String, ImmutableWikicode)>> + 'r,
        ReplacerList: CategoryReplacerList + 'r;

    fn params_to_wikitext(
        self,
        bot: &Bot,
    ) -> impl Stream<Item = anyhow::Result<(String, Option<String>)>>
    where
        Self: Stream<Item = anyhow::Result<(String, Option<ImmutableWikicode>)>>;
}

impl<S> CustomStreamExt for S
where
    S: Stream,
{
    fn params_to_html(
        self,
        bot: &Bot,
    ) -> impl Stream<Item = anyhow::Result<(String, ImmutableWikicode)>>
    where
        Self: Stream<Item = (String, String)>,
    {
        self.map(|(k, v)| {
            let bot = bot.clone();
            tokio::spawn(async move {
                let v = bot.parsoid().transform_to_html(&v).await?;

                Ok::<_, anyhow::Error>((k, v))
            })
        })
        .then(|handle| async { handle.await? })
    }

    fn replace_params<'r, ReplacerList>(
        self,
        replacer: &'r RecursionReplacer<ReplacerList>,
    ) -> impl Stream<Item = anyhow::Result<(String, Option<ImmutableWikicode>)>> + 'r
    where
        Self: Stream<Item = anyhow::Result<(String, ImmutableWikicode)>> + 'r,
        ReplacerList: CategoryReplacerList + 'r,
    {
        self.and_then(move |(k, v)| async move {
            let replaced = replacer.replace(v).await?;

            Ok((k, replaced))
        })
    }

    fn params_to_wikitext(
        self,
        bot: &Bot,
    ) -> impl Stream<Item = anyhow::Result<(String, Option<String>)>>
    where
        Self: Stream<Item = anyhow::Result<(String, Option<ImmutableWikicode>)>>,
    {
        self.and_then(|(k, v)| async {
            let bot = bot.clone();

            let v = tokio::spawn(async move {
                match v {
                    Some(v) => anyhow::Ok(Some(bot.parsoid().transform_to_wikitext(&v).await?)),
                    None => Ok(None),
                }
            })
            .await??;

            Ok::<_, anyhow::Error>((k, v))
        })
    }
}

#[cfg(test)]
mod tests {
    use frunk_core::hlist;
    use indoc::indoc;

    use super::*;
    use crate::replacer::template::image_requested::ImageRequestedReplacer;
    use crate::util::test;

    #[tokio::test]
    async fn test_nested_template() -> anyhow::Result<()> {
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

        let html = bot.parsoid().transform_to_html(before).await?;

        let replacer = hlist![RecursionReplacer::new(
            bot.clone(),
            hlist![ImageRequestedReplacer::new(from, to)],
        )];
        let (replaced_html, is_changed) = replacer.replace_all(html).await?;

        assert!(is_changed);

        let replaced_wikicode = bot.parsoid().transform_to_wikitext(&replaced_html).await?;

        assert_eq!(after, replaced_wikicode);

        Ok(())
    }
}
