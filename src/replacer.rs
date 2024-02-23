use std::fmt::Debug;
use std::future::Future;

use derivative::Derivative;
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

pub trait CategoryReplacer: Send + Sync {
    fn replace(
        &self,
        html: ImmutableWikicode,
    ) -> impl Future<Output = anyhow::Result<Option<ImmutableWikicode>>> + Send + Sync;

    fn boxed(self) -> BoxedCategoryReplacer<Self>
    where
        Self: Sized,
    {
        BoxedCategoryReplacer::new(self)
    }
}

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

#[derive(Derivative)]
#[derivative(Clone)]
pub struct BoxedCategoryReplacer<Replacer> {
    #[derivative(Clone(bound = "Replacer: Clone"))]
    inner: Replacer,
}

impl<Replacer> BoxedCategoryReplacer<Replacer>
where
    Replacer: CategoryReplacer,
{
    pub fn new(inner: Replacer) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> Replacer {
        self.inner
    }
}

impl<Replacer> CategoryReplacer for BoxedCategoryReplacer<Replacer>
where
    Replacer: CategoryReplacer,
{
    fn replace(
        &self,
        html: ImmutableWikicode,
    ) -> impl Future<Output = anyhow::Result<Option<ImmutableWikicode>>> + Send + Sync {
        Box::pin(self.inner.replace(html))
    }
}

pub trait CategoryReplacerList: Send + Sync {
    /// If the result of the substitution is the same as the original ImmutableWikicode,
    /// the bool in the return tuple returns false
    fn replace_all(
        &self,
        html: ImmutableWikicode,
    ) -> impl Future<Output = anyhow::Result<(ImmutableWikicode, bool)>> + Send + Sync;
}

impl CategoryReplacerList for HNil {
    async fn replace_all(
        &self,
        html: ImmutableWikicode,
    ) -> anyhow::Result<(ImmutableWikicode, bool)> {
        Ok((html, false))
    }
}

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
