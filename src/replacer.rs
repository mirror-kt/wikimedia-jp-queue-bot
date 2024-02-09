use std::fmt::Debug;

use async_trait::async_trait;
use frunk_core::hlist;
use frunk_core::hlist::{HCons, HNil};
use mwbot::parsoid::prelude::*;
use mwbot::Bot;

use self::category_tag::CategoryTagReplacer;
use self::recursion::RecursionReplacer;
use self::template::category_of_redirects::CategoryOfRedirectsReplacer;
use self::template::image_requested::ImageRequestedReplacer;

mod category_tag;
mod recursion;
mod template;

#[async_trait]
pub trait CategoryReplacer: Send + Sync {
    async fn replace(&self, html: ImmutableWikicode) -> anyhow::Result<Option<ImmutableWikicode>>;
}

#[async_trait]
impl<Replacer> CategoryReplacer for Option<Replacer>
where
    Replacer: CategoryReplacer,
{
    async fn replace(&self, html: ImmutableWikicode) -> anyhow::Result<Option<ImmutableWikicode>> {
        match self {
            Some(replacer) => replacer.replace(html).await,
            None => Ok(None),
        }
    }
}

#[async_trait]
pub trait CategoryReplacerList: Send + Sync {
    /// If the result of the substitution is the same as the original ImmutableWikicode,
    /// the bool in the return tuple returns false
    async fn replace_all(
        &self,
        html: ImmutableWikicode,
    ) -> anyhow::Result<(ImmutableWikicode, bool)>;
}

#[async_trait]
impl CategoryReplacerList for HNil {
    async fn replace_all(
        &self,
        html: ImmutableWikicode,
    ) -> anyhow::Result<(ImmutableWikicode, bool)> {
        Ok((html, false))
    }
}

#[async_trait]
impl<Replacer, ReplacerList> CategoryReplacerList for HCons<Replacer, ReplacerList>
where
    Replacer: CategoryReplacer,
    ReplacerList: CategoryReplacerList,
{
    async fn replace_all(
        &self,
        html: ImmutableWikicode,
    ) -> anyhow::Result<(ImmutableWikicode, bool)> {
        let replaced = self.head.replace(html.clone()).await?;
        let head_is_changed = replaced.is_some();
        let (tail_replaced, tail_is_changed) =
            self.tail.replace_all(replaced.unwrap_or(html)).await?;

        Ok((tail_replaced, head_is_changed || tail_is_changed))
    }
}

pub fn get_category_replacers(
    bot: Bot,
    from: String,
    to: Vec<String>,
) -> impl CategoryReplacerList + Debug {
    hlist![RecursionReplacer::new(
        bot,
        hlist![
            CategoryTagReplacer::new(from.clone(), to.clone()),
            CategoryOfRedirectsReplacer::new(from.clone(), to.clone()),
            ImageRequestedReplacer::new(from, to),
        ],
    )]
}
