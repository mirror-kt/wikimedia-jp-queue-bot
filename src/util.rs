use kuchiki::NodeRef;
use markup5ever::interface::QualName;
use markup5ever::Namespace;
use mwbot::parsoid::prelude::*;

pub trait ListExt {
    /// 順序なしリスト
    fn collect_to_ul(self) -> Wikicode;

    /// 順序有りリスト
    fn collect_to_ol(self) -> Wikicode;
}

impl<T: Iterator<Item = String>> ListExt for T {
    fn collect_to_ul(self) -> Wikicode {
        let wikicode = Wikicode::new_node("ul");
        self.for_each(|c| {
            let li = Wikicode::new_node("li");
            li.append(&Wikicode::new_text(&c));
            li.append(&Wikicode::new_text("\n"));
            wikicode.append(&li);
        });
        wikicode
    }

    fn collect_to_ol(self) -> Wikicode {
        let wikicode = Wikicode::new_node("ol");
        self.for_each(|c| {
            let li = Wikicode::new_node("li");
            li.append(&Wikicode::new_text(&c));
            li.append(&Wikicode::new_text("\n"));
            wikicode.append(&li);
        });
        wikicode
    }
}
