use std::fmt::Debug;

use derivative::Derivative;
use indexmap::IndexMap;
use mwbot::parsoid::prelude::*;
use mwbot::{Bot, Page, SaveOptions};
use tracing::{info, warn};
use ulid::Ulid;

use crate::db::{store_command, store_operation, CommandType};
use crate::generator::list_category_members;
use crate::is_emergency_stopped;
use crate::replacer::CategoryReplacerList;

pub mod parse;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Command<R> {
    bot: Bot,
    dry_run: bool,
    pub(crate) id: Ulid,
    pub(crate) from: String,
    pub(crate) to: Vec<String>,
    pub(crate) discussion_link: String,
    pub(crate) namespaces: Vec<u32>,
    replacers: R,
    #[derivative(Debug = "ignore")]
    save_opts: SaveOptions,
    pub(crate) command_type: CommandType,
}

impl<R> Command<R>
where
    R: CategoryReplacerList + Debug,
{
    pub async fn execute(self) -> CommandStatus {
        if let Err(err) = store_command(&self).await {
            return CommandStatus::Error {
                id: Ulid::new(),
                statuses: IndexMap::new(),
                message: format!("コマンドをデータベースに保存できませんでした: {:?}", err),
            };
        }

        let mut category_members =
            list_category_members(&self.bot, &self.from, self.namespaces.clone()).await;

        let mut statuses = IndexMap::new();
        while let Some(page) = category_members.recv().await {
            if is_emergency_stopped(&self.bot).await {
                return CommandStatus::EmergencyStopped;
            }

            let Ok(page) = page else {
                warn!("Error while getting: {:?}", page);
                continue;
            };
            statuses.insert(page.title().to_string(), self.process_page(page).await);
        }

        if statuses.is_empty() {
            CommandStatus::CategoryEmpty
        } else {
            CommandStatus::Done {
                id: self.id,
                statuses,
            }
        }
    }

    async fn process_page(&self, page: Page) -> OperationResult {
        let html = page.html().await.map_err(|err| {
            warn!(message = "ページの取得中にエラーが発生しました", err = ?err);
            "ページの取得中にエラーが発生しました".to_string()
        })?;

        let (replaced, is_changed) = self.replacers.replace_all(html).await.map_err(|err| {
            warn!(message = "カテゴリの変更中にエラーが発生しました", err = ?err);
            "カテゴリの変更中にエラーが発生しました".to_string()
        })?;

        if !is_changed {
            return Ok(OperationStatus::Skipped);
        }

        self.save_page(page, replaced).await?;

        Ok(OperationStatus::Done)
    }

    async fn save_page<S>(&self, page: Page, edit: S) -> Result<Page, String>
    where
        S: Into<ImmutableWikicode>,
    {
        if self.dry_run {
            info!("No save was made due to dry-run");
            return Ok(page);
        }

        let (page, res) = page
            .save(edit.into(), &self.save_opts)
            .await
            .map_err(|err| {
                warn!(message = "ページの保存に失敗しました", err = ?err);
                "ページの保存に失敗しました".to_string()
            })?;
        self.store_operation_to_db(res.pageid, res.newrevid).await?;

        Ok(page)
    }

    async fn store_operation_to_db(
        &self,
        page_id: u32,
        new_rev_id: Option<u64>,
    ) -> Result<(), String> {
        let Some(new_rev_id) = new_rev_id else {
            return Err("新しい版のIDを取得できませんでした".to_string());
        };

        store_operation(&self.id, page_id, new_rev_id)
            .await
            .map_err(|err| {
                warn!(message = "データベースへのオペレーション保存に失敗しました", err = ?err);
                "データベースへのオペレーション保存に失敗しました".to_string()
            })
    }
}

#[derive(Debug)]
pub enum CommandStatus {
    EmergencyStopped,
    Done {
        id: Ulid,
        statuses: IndexMap<String, OperationResult>,
    },
    /// Commandがエラーの場合
    Error {
        id: Ulid,
        /// <title, status>
        statuses: IndexMap<String, OperationResult>,
        message: String,
    },
    Skipped,
    CategoryEmpty,
}

#[derive(Debug, PartialEq)]
pub enum OperationStatus {
    Done,
    Skipped,
}

pub type OperationResult = Result<OperationStatus, String>;
