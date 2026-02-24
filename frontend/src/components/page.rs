use gloo_storage::{LocalStorage, Storage};
use gloo_utils::document;
use wasm_bindgen::UnwrapThrowExt;
use web_sys::HtmlButtonElement;
use crate::{api::ApiClient, components::{Route, lib::{clear, root_append, root_clear}}, state::State};
use html_to_string_macro::html;
use crate::bind;
use log::debug;

const THEME_KEY: &str = "theme"; // "light" | "dark"

enum Theme {
    Light,
    Dark,
}

impl Theme {
    fn get_from_storage() -> Self {
        match LocalStorage::get::<String>(THEME_KEY).unwrap_or_else(|_| "dark".to_string()).as_str() {
            "light" => Theme::Light,
            _ => Theme::Dark,
        }
    }

    fn next(&self) -> Theme {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }

    fn class(&self) -> &'static str {
        match self {
            Theme::Light => "theme-light",
            Theme::Dark => "",
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Theme::Light => "Light theme",
            Theme::Dark => "Dark theme",
        }
    }
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

fn apply_theme(theme: Theme) {
    let doc = web_sys::window().and_then(|w| w.document()).expect_throw("X017: document");
    let root = doc.document_element().expect_throw("X018: documentElement");
    crate::bind::log_on_err(match theme {
        Theme::Dark => root.class_list().remove_1(Theme::Light.class()),
        Theme::Light => root.class_list().add_1(Theme::Light.class()),
    });
    doc.get_element_by_id("toggle-theme").map(|el| el.set_text_content(Some(theme.label())));
}

pub fn append_navbar() {
    let current = Theme::get_from_storage();

    debug!("Router initialized 2.1 page being set up");
    let h = html! {
        <div id="navbar">
            <div class="container">
                <div class="toolbar">
                    <button class="button ghost" id="nav-entries">{ "Entries" }</button>
                    <button class="button ghost" id="nav-setup">{ "Setup" }</button>
                </div>
                <div class="toolbar">
                    <button class="button" id="toggle-theme">{ current.label() }</button>
                    <button class="button" id="logout">{ "Logout" }</button>
                </div>
            </div>
        </div>
    };
    root_append(h.as_str());


    debug!("Router initialized 2.2 page being set up");
    let toggle_theme = move |_| {
        let cur = Theme::get_from_storage();
        let next = cur.next();
        let _ = LocalStorage::set(THEME_KEY, next.as_str());
        apply_theme(next);
    };
    debug!("Router initialized 2.3 page being set up");
    let logout = move |_| {
        State::clear_token();
        Route::Login.visit();
    };

    debug!("Router initialized 2.4 page being set up");
    let logout_button = bind::get_element_by_id::<HtmlButtonElement>("logout");
    let toggle_theme_button = bind::get_element_by_id::<HtmlButtonElement>("toggle-theme");
    let entries_button = bind::get_element_by_id::<HtmlButtonElement>("nav-entries");
    let setup_button = bind::get_element_by_id::<HtmlButtonElement>("nav-setup");
    debug!("Router initialized 2.5 page being set up");
    bind::set_onclick(&toggle_theme_button, toggle_theme);
    bind::set_onclick(&logout_button, logout);
    // TODO(1): THIRD WAY
    bind::set_onclick(&entries_button, move |_| {
        Route::Entries.visit();
    });
    bind::set_onclick(&setup_button, move |_| {
        Route::Setup.visit();
    });
}

pub fn empty_page() {
    root_clear();
    root_append("<div id=\"page\"></div>");
    let theme = LocalStorage::get::<String>(THEME_KEY).unwrap_or_else(|_| "dark".to_string());
    apply_theme(if theme == "light" { Theme::Light } else { Theme::Dark });
}

pub fn navbar_page() {
    // TODO(10): When navigating, instead of clearing the previous page, the router should cache it in some DOM cache before deleting it.
    // When re-rendering the same page, it should rebuild it from the DOM cache.
    // The cache should also contain the latest timestamps used to render that DOM, so we know what information
    // to compare to in the timestamp-based poll.
    // A timestamp-based poll should be implemented on the server side for currencies, sources, categories,
    // and entries, which the client side should use to re-fetch any APIs that may have changed results from
    // another device.
    // What counts as changed: select all entries / currencies / sources / categories where create date or update date
    // are greater than the last update timestamp
    // Reconciliate those by merging updates into existing elements, and adding creates as new elements in lists
    match document().get_element_by_id("navbar") {
        Some(_) => clear(),
        None =>  {
            root_clear();
            append_navbar();
            root_append("<div id=\"page\"></div>");
        }
    }
    apply_theme(Theme::get_from_storage());
}