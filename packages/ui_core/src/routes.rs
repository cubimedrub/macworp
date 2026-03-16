use dioxus::prelude::*;
use dioxus::router::Routable;

use crate::layouts::two_pane::TwoPane;
use crate::pages::*;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Routes {
    #[layout(TwoPane)]
        #[route("/")]
        Home {},
}
