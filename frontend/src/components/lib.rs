use crate::{bind, router::Router};

pub fn append(html: &str) {
    bind::page().insert_adjacent_html("beforeend", html).expect("X012: insert_adjacent_html failed in render");
}

pub fn replace(html: &str) {
    bind::page().set_inner_html(html);
}

pub fn clear() {
    bind::page().set_inner_html("");
}

pub fn root_append(html: &str) {
    bind::root().insert_adjacent_html("beforeend", html).expect("X012: insert_adjacent_html failed in render");
}

pub fn root_replace(html: &str) {
    bind::root().set_inner_html(html);
}

pub fn root_clear() {
    bind::root().set_inner_html("");
}


#[derive(Clone, PartialEq)]
pub enum Route {
    Empty,
    Root,
    Login,
    Setup,
    Entries,
    NotFound,
}

impl Route {
    pub fn as_str(&self) -> &'static str {
        match self {
            Route::Empty => "",
            Route::Root => "/",
            Route::Login => "/login",
            Route::Setup => "/setup",
            Route::Entries => "/entries",
            Route::NotFound => "404", // No slash as router expects no slash
        }
    }

    /// Visiting with a route visits with the singleton router
    pub fn visit(&self) {
        Router::get().visit(self.as_str());
    }
}

impl Into<String> for Route {
    fn into(self) -> String {
        self.as_str().into()
    }
}

impl<'a> Into<&'a str> for Route {
    fn into(self) -> &'a str {
        self.as_str()
    }
}

impl std::fmt::Display for Route {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
