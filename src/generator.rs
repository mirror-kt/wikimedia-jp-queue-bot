use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use mwbot::generators::{CategoryMembers, Generator, Search};
use mwbot::{Bot, Page, Result};
use tokio::sync::mpsc::{self, Receiver, Sender};

/// カテゴリに所属する全ページを返す.
/// [`CategoryMembers`] だけでは{{リダイレクトの所属カテゴリ}}などが取得できない.
pub async fn list_category_members(
    bot: &Bot,
    category: impl Into<String>,
    namespaces: Vec<u32>,
) -> Receiver<Result<Page>> {
    let (tx, rx) = mpsc::channel(50);

    let category = category.into();
    let bot = bot.clone();

    let seen = Arc::new(Mutex::new(HashSet::<String>::new()));

    let category_members = CategoryMembers::new(category.clone())
        .namespace(namespaces.clone())
        .generate(&bot);
    send_categories(category_members, tx.clone(), seen.clone()).await;

    let search = Search::new(format!(r#"insource:"{}""#, category.clone()))
        .namespace(namespaces.clone())
        .generate(&bot);
    send_categories(search, tx.clone(), seen.clone()).await;

    rx
}

async fn send_categories(
    mut recv: Receiver<Result<Page>>,
    tx: Sender<Result<Page>>,
    seen: Arc<Mutex<HashSet<String>>>,
) {
    tokio::spawn(async move {
        while let Some(member) = recv.recv().await {
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
