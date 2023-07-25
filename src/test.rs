use std::path::Path;

use mwbot::Bot;

pub async fn bot() -> Bot {
    Bot::from_path(&Path::new("./mwbot.test.toml")).await.unwrap()
}