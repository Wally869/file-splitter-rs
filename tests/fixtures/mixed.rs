//! Mixed fixture: the integration tests split this file and compare its items.

use std::fmt;

pub struct Panel {
    width: u32,
}

extern crate alloc;

pub union Word {
    pub int: u32,
    pub float: f32,
}

unsafe extern "C" {
    fn abs(input: i32) -> i32;
}

macro_rules! square {
    ($value:expr) => {
        $value * $value
    };
}

static LIMIT: u32 = 10;

// comment: between LIMIT and SCALE
const SCALE: u32 = 3;

type Size = u32;

pub enum Mode {
    Wide,
    Narrow,
}

pub trait Render {
    fn render(&self) -> String;
}

impl Panel {
    const MARGIN: u32 = 2;

    fn panel_banner(&self) -> &'static str {
        "top method
  second line
            third line"
    }

    // comment: before panel_raw
    fn panel_raw(&self) -> &'static str {
        r#"top "raw" method
      indented
end"#
    }
}

impl fmt::Display for Panel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.width)
    }
}

// comment: before free_banner
fn free_banner() -> &'static str {
    "top fn
  second line
            third line"
}

fn free_raw() -> &'static str {
    r#"top "raw" fn
      indented
end"#
}

fn path_text() -> String {
    // super::x stays as written
    let real = super::Outer::NAME;
    format!("{}{}{}", "super::x", r"super::", real)
}

pub fn measure(panel: &Panel) -> Size {
    square!(panel.width) * SCALE + LIMIT + Panel::MARGIN + unsafe { abs(-1) as u32 }
}

mod inner {
    use super::*;

    pub struct Card;

    fn inner_banner() -> &'static str {
        "inner fn
  second line
            third line"
    }

    fn inner_raw() -> &'static str {
        r#"inner "raw" fn
      indented
end"#
    }

    impl Card {
        fn card_banner(&self) -> &'static str {
            "inner method
  second line
            third line"
        }

        fn card_raw(&self) -> &'static str {
            r#"inner "raw" method
      indented
end"#
        }
    }

    impl Render for Card {
        fn render(&self) -> String {
            inner_banner().to_string()
        }
    }
}

// comment: end of file
