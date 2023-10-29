use std::future::Future;

use chrono::{DateTime, Utc};
use mwbot::parsoid::prelude::*;

pub trait IterExt {
    /// 順序なしリスト
    fn collect_to_ul(self) -> Wikicode;

    /// 順序有りリスト
    fn collect_to_ol(self) -> Wikicode;
}

impl<T: Iterator<Item = Wikicode>> IterExt for T {
    fn collect_to_ul(self) -> Wikicode {
        let wikicode = Wikicode::new_node("ul");
        self.for_each(|c| {
            let li = Wikicode::new_node("li");
            li.append(&c);
            li.append(&Wikicode::new_text("\n"));
            wikicode.append(&li);
        });
        wikicode
    }

    fn collect_to_ol(self) -> Wikicode {
        let wikicode = Wikicode::new_node("ol");
        self.for_each(|c| {
            let li = Wikicode::new_node("li");
            li.append(&c);
            li.append(&Wikicode::new_text("\n"));
            wikicode.append(&li);
        });
        wikicode
    }
}

pub trait IntoWikicode {
    fn as_wikicode(&self) -> Wikicode;
}

impl IntoWikicode for Wikinode {
    fn as_wikicode(&self) -> Wikicode {
        let wikicode = Wikicode::new("");
        wikicode.append(self);

        wikicode
    }
}

impl IntoWikicode for Vec<Wikinode> {
    fn as_wikicode(&self) -> Wikicode {
        let wikicode = Wikicode::new("");
        self.iter().for_each(|c| {
            wikicode.append(c);
        });
        wikicode
    }
}

impl IntoWikicode for String {
    fn as_wikicode(&self) -> Wikicode {
        Wikicode::new_text(self)
    }
}

pub trait DateTimeProvider {
    fn now(&self) -> DateTime<Utc>;
}

pub struct UtcDateTimeProvider;
impl DateTimeProvider for UtcDateTimeProvider {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
pub mod test {
    use std::path::Path;

    use mwbot::Bot;

    pub async fn bot() -> Bot {
        Bot::from_path(Path::new("./mwbot.test.toml"))
            .await
            .unwrap()
    }
}
