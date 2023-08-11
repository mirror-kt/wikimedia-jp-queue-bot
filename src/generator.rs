use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use mwbot::generators::{CategoryMembers, Generator, Search};
use mwbot::{Bot, Page, Result};
use tokio::sync::mpsc::{self, Receiver};

/// カテゴリに所属する全ページを返す.
/// [`mwbot::generators::CategoryMembers`] だけでは{{リダイレクトの所属カテゴリ}}などが取得できない.
pub fn list_category_members<'category>(
    bot: &Bot,
    category: impl Into<Cow<'category, str>>,
    include_article: bool,
    include_category: bool,
) -> Receiver<Result<Page>> {
    let (tx, rx) = mpsc::channel(50);

    let category = category.into().into_owned();
    let bot = bot.clone();

    let mut namespaces = Vec::new();
    if include_article {
        namespaces.push(0); // 標準名前空間
    }
    if include_category {
        namespaces.push(14); // Category名前空間
    }

    let seen = Arc::new(Mutex::new(HashSet::<String>::new()));

    {
        let bot = bot.clone();
        let category = category.clone();
        let namespaces = namespaces.clone();
        let tx = tx.clone();
        let seen = Arc::clone(&seen);

        tokio::spawn(async move {
            let mut category_members = CategoryMembers::new(category)
                .namespace(namespaces)
                .generate(&bot);
            while let Some(member) = category_members.recv().await {
                {
                    let mut seen = seen.lock().unwrap();
                    if let Ok(member) = &member {
                        if seen.contains(member.title()) {
                            continue;
                        }

                        seen.insert(member.title().to_string());
                    }
                }

                if tx.send(member).await.is_err() {
                    // Receiver hung up, just abort
                    return;
                }
            }
        });
    }

    {
        let bot = bot.clone();
        let category = category.clone();
        let namespaces = namespaces.clone();
        let tx = tx.clone();
        let seen = Arc::clone(&seen);
        tokio::spawn(async move {
            let mut search = Search::new(format!(r#"insource:"{}""#, category))
                .namespace(namespaces)
                .generate(&bot);
            while let Some(member) = search.recv().await {
                {
                    let mut seen = seen.lock().unwrap();
                    if let Ok(member) = &member {
                        if seen.contains(member.title()) {
                            continue;
                        }

                        seen.insert(member.title().to_string());
                    }
                }

                if tx.send(member).await.is_err() {
                    // Receiver hung up, just abort
                    return;
                }
            }
        });
    }

    rx
}
