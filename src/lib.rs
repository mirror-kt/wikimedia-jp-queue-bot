pub mod command;
pub mod consume;

use kuchiki::NodeRef;
use mwbot::parsoid::{prelude::Wikinode, Wikicode};

pub trait IntoWikicode {
    fn into_wikicode(&self) -> Wikicode;
}

impl IntoWikicode for NodeRef {
    fn into_wikicode(&self) -> Wikicode {
        Wikicode::new_node(&self.to_string())
    }
}

impl IntoWikicode for Wikinode {
    fn into_wikicode(&self) -> Wikicode {
        Wikicode::new(&self.to_string())
    }
}

impl IntoWikicode for Vec<Wikinode> {
    fn into_wikicode(&self) -> Wikicode {
        Wikicode::new(&self.iter().map(|node| node.to_string()).collect::<String>())
    }
}

impl IntoWikicode for String {
    fn into_wikicode(&self) -> Wikicode {
        Wikicode::new_text(self)
    }
}
