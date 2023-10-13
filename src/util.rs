use kuchiki::NodeRef;
use mwbot::parsoid::prelude::*;

pub trait IterExt {
    /// 順序なしリスト
    fn collect_to_ul(self) -> Wikicode;

    /// 順序有りリスト
    fn collect_to_ol(self) -> Wikicode;
}

impl<T: Iterator<Item=Wikicode>> IterExt for T {
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

impl IntoWikicode for NodeRef {
    fn as_wikicode(&self) -> Wikicode {
        Wikicode::new_node(&self.to_string())
    }
}

impl IntoWikicode for Wikinode {
    fn as_wikicode(&self) -> Wikicode {
        Wikicode::new(&self.to_string())
    }
}

impl IntoWikicode for Vec<Wikinode> {
    fn as_wikicode(&self) -> Wikicode {
        Wikicode::new(&self.iter().map(|node| node.to_string()).collect::<String>())
    }
}

impl IntoWikicode for String {
    fn as_wikicode(&self) -> Wikicode {
        Wikicode::new_text(self)
    }
}
