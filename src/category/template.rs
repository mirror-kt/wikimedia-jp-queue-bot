use async_recursion::async_recursion;
use futures_util::future::JoinAll;
use indexmap19::IndexMap;
use mwbot::parsoid::prelude::*;
use mwbot::Bot;

pub(super) mod category_of_redirects;
pub(super) mod image_wanted;

/// テンプレートの再帰的な置換
/// Parsoidを複数回呼び出すため、このメソッドを複数回呼ぶよりも置換関数をまとめて1回だけ呼び出すほうがよい
#[async_recursion(?Send)]
pub async fn replace_recursion<F: Fn(&Wikicode, &str, &[String]) -> anyhow::Result<bool>>(
    bot: &Bot,
    html: &Wikicode,
    from: &str,
    to: &[String],
    replace_fn: &F,
) -> anyhow::Result<bool> {
    let mut is_changed = replace_fn(html, from, to)?;

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
            .collect::<JoinAll<_>>()
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
                    match replace_recursion(bot, &parsed_v, from, to, replace_fn).await {
                        Ok(true) => Ok((parsed_v.into_immutable(), k, v)),
                        _ => Err((k, v)),
                    }
                }
            })
            .collect::<JoinAll<_>>()
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
            .collect::<JoinAll<_>>()
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
