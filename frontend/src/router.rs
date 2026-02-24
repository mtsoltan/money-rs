// Modified from https://github.com/arwinneil/wasm-router/blob/master/src/router.rs
use gloo_history::{BrowserHistory, History};
use gloo_utils::window;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use log::{debug, warn};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::bind;
use crate::state::State;

pub enum MiddlewareResult {
    Content(String),
    Route(String),
}

// TODO(1): Use this
pub struct Route {
    /// The URL that visiting this route would set
    /// In reality, multiple URLs can be mapped to visit the same route,
    /// so this route-url relation can only be used to set the url from the route
    /// and not necessarily vice-versa.
    url: String,
    /// Converts content into another content it returns, usually by appending.
    /// Middleware can also change the route entirely (for example, auth middleware sending to login)
    /// If any of the middleware stack changes the route, the entire content and middleware stack is dropped.
    middleware: Vec<fn(path: &str, content: &str) -> MiddlewareResult>,
    /// Displays the content. Most renderers would just replace the content of the entire page, or replace the
    /// content of the entire page aside from headers and footers like nav-bars.
    renderer: Box<fn(path: &str, content: &str)>,
}

// TODO(1): make back navigation work

#[derive(Clone)]
pub struct Router {
    prefix: &'static str,
    routes: Rc<RefCell<HashMap<String, fn(url: &str, state: Cow<State>)>>>,
    hooks: Rc<RefCell<HashMap<String, fn()>>>,
    history: BrowserHistory,
}

impl Router {
    thread_local! {
        static ROUTER: Router = {
            Router {
                prefix: "/app", // TODO(40): Find a way to initialize this at compile time from main
                routes: Rc::new(RefCell::new(HashMap::new())),
                hooks: Rc::new(RefCell::new(HashMap::new())),
                history: BrowserHistory::new(),
            }
        };
    }

    fn normalize_path<'a>(&self, l: &'a str) -> &'a str {
        l.strip_prefix(self.prefix).unwrap_or(l)
    }

    pub fn new(_prefix: &'static str) -> Router {
        Self::ROUTER.with(|r| r.clone())
    }

    pub fn get() -> Router {
        Self::ROUTER.with(|r| r.clone())
    }

    pub fn init(&self) {
        // Route initial URL
        self.run_hook("on_load_start");
        debug!("Router initialized 0");
        let loc = bind::location();
        debug!("Router initialized 1 {}", self.normalize_path(loc.as_str()));
        self.route(loc.as_str());

        // Routing for navigating in history and escaping hash routes
        let handle_pop = Closure::wrap(Box::new(move |e: web_sys::Event| {
            let router = Router::get();
            let loc = router.history.location();
            let path = loc.path();

            debug!("pop handle: {}", path);

            Router::get().route(path)
        }) as Box<dyn Fn(_)>);

        window().set_onpopstate(Some(handle_pop.as_ref().unchecked_ref()));
        handle_pop.forget();
        debug!("Router initialized 5");
        self.run_hook("on_load_end");
    }

    pub fn add<T: Into<String>>(&mut self, route: T, handler: fn(url: &str, state: Cow<State>)) -> &mut Self {
        self.routes.borrow_mut().insert(route.into(), handler);
        self
    }

    pub fn remove(&mut self, route: &str) -> &mut Self {
        self.routes.borrow_mut().remove(route);
        self
    }

    /// pub fn loaded() {
    ///     bind::document().get_element_by_id("body").unwrap()
    ///         .dyn_ref::<Element>().unwrap().class_list().remove_1("loading");
    /// }
    /// r.add_hook("on_load_end", loaded);
    pub fn add_hook(&mut self, hook: &str, handler: fn()) -> &mut Self {
        self.hooks.borrow_mut().insert(String::from(hook), handler);
        self
    }

    fn run_hook(&self, hook: &str) {

        match self.hooks.borrow().get(hook) {
            Some(handler) => {
                debug!("{}", hook);
                handler()
            },
            None => (),
        }
    }

    pub fn route(&self, destination: &str) {
        let destination = self.normalize_path(destination);
        debug!("Router initialized 2 {:?}, {}", self.routes, destination);
        let routes = self.routes.clone();
        self.run_hook("on_route_start");
        // Read state from browser history
        let state = serde_wasm_bindgen::from_value::<State>(
            bind::history().state().expect_throw("X022: Failed to deserialize state")
        ).expect_throw("X022: Failed to deserialize state");
        match routes.borrow().get(destination) {
            Some(route_handler) => route_handler(destination, Cow::Borrowed(&state)),
            None => match routes.borrow().get("404") {
                Some(route_handler) => route_handler(destination, Cow::Borrowed(&state)),
                None => {
                    warn!("Page not found!");
                }
            },
        }
        self.run_hook("on_route_end");
        debug!("Router initialized 4");
    }

    /// Sets the address bar location, pushing the history, then routes.
    /// TODO(40): Support pushing state to history using push_with_state some way or another.
    /// It is static, but I don't know of a way to get &'static T from LocalKey<Rc<RefCell<T>>>
    pub fn visit(&self, location: &str) {
        let loc = format!("{}{}", self.prefix, location);
        self.history.push(&loc);
        self.route(location);
    }
}
