use std::borrow::Cow;

use html_to_string_macro::html;
use crate::{components::{empty_page, lib::replace}, state::State};
use log::debug;

pub fn not_found(_: &str, _: Cow<State>) {
    // TODO(1): Test actual page reload
    empty_page();
    debug!("Router initialized 3 not found");
    let h = html! {
        <div class="container stack">
            <h2>{ "Not found" }</h2>
            <a href={crate::components::lib::Route::Root} classes="button">"Go to Home"</a>
        </div>
    };
    replace(h.as_str());
}
