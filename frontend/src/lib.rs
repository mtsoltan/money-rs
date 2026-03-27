mod router;
mod api;
mod state;
mod date;
mod components;
mod bind;
mod consts;

use wasm_bindgen::prelude::wasm_bindgen;
use crate::bind::init_wasm_logger;
use crate::router::Router;
use crate::components::Route;

#[wasm_bindgen(start)]
pub fn start() {
    init_wasm_logger();
    Router::new("/app")
        // .add_hook("on_load_start", components::init_page)
        .add(Route::Empty, components::login_or_entries)
        .add(Route::Root, components::login_or_entries)
        .add(Route::Login, components::login)
        .add(Route::Setup, components::setup)
        .add(Route::Entries, components::entries)
        .add(Route::NotFound, components::not_found)
        .init();
    }
