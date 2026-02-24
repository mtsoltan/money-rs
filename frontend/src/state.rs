use std::{cell::RefCell, rc::Rc};

use gloo_storage::{LocalStorage, Storage};
use model::entity::EntryQuery;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use wasm_bindgen::UnwrapThrowExt;

// TODO(30): Clean up this file and use the state in it in other components
// TODO(30): Use results and propagate errors instead of `expect` and `expect_throw` everywhere
// ApiError type exists, make a similar storage error

trait LocalStorageGetter<T> where T: Default + DeserializeOwned + Serialize {
    fn value() -> T {
        match LocalStorage::get::<T>(Self::key()) {
            Ok(s) => Ok(s),
            Err(gloo_storage::errors::StorageError::KeyNotFound(_)) => Ok(T::default()),
            Err(e) => Err(e),
        }.expect_throw(format!("X022: Could not deserialize {}", Self::key()).as_str())
    }

    fn clear() {
        let _ = LocalStorage::delete(Self::key());
    }

    fn key() -> &'static str;
}

trait LocalStorageSetter<V> {
    fn set(value: V);
}

impl<T, R> LocalStorageSetter<Rc<RefCell<T>>> for R where
R: LocalStorageGetter<T>,
T: Default + DeserializeOwned + Serialize {
    fn set(value: Rc<RefCell<T>>) {
        LocalStorage::set(Self::key(), &*value)
            .expect_throw(format!("X023: Could not set {}", Self::key()).as_str());
    }

}

struct LocalStorageState;

impl LocalStorageGetter<State> for LocalStorageState {
    fn key() -> &'static str { "state" }
}

struct LocalStorageToken;

impl LocalStorageGetter<Option<String>> for LocalStorageToken {
    fn key() -> &'static str { "token" }
}

impl LocalStorageSetter<String> for LocalStorageToken {
    fn set(value: String) {
        Self::set(value.as_str());
    }

}

impl LocalStorageSetter<&str> for LocalStorageToken {
    fn set(value: &str) {
        LocalStorage::set(Self::key(), value)
            .expect_throw(format!("X023: Could not set {}", Self::key()).as_str());
    }

}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientSidePagination {
    pub page_size: u32,
    pub page: u32,
}

impl Default for ClientSidePagination {
    fn default() -> Self {
        Self {
            page_size: 100,
            page: 0,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct State {
    /// This should almost always be true. We use this system instead of an enum type
    /// so that we do not have to match over it every time we need to use it.
    /// See Self::get_state for more details on how this is sanitized.
    synced: bool,
    /// Defaults to None, using the fixed currency from backend
    pub display_currency: Option<String>,
    /// Whether the filtering will do server-side search or client-side filtering
    /// Defaults to false (server-side)
    pub client_side_filters: bool,
    /// Filters for client-side, similar to server-side query
    /// Defaults to empty query
    pub filters: EntryQuery,
    pub pagination: ClientSidePagination,
}

impl State {
    thread_local! {
        static STATE: Rc<RefCell<State>> = Rc::default();
        static TOKEN: Rc<RefCell<Option<String>>> = Rc::default();
    }

    /// See comments inside [`Self::get_state`] to understand logic.
    pub fn get_token() -> Option<String> {
        let from_static = Self::TOKEN.with(|r| r.clone());
        if (*from_static).borrow().is_none() {
            Self::TOKEN.with(|r| r.replace(LocalStorageToken::value()));
        }
        Self::TOKEN.with(|r| r.clone().borrow().clone())
    }

    pub fn set_token(token: &str) {
        Self::TOKEN.with(|r| r.replace(Some(token.to_string())));
        LocalStorageToken::set(token);
    }

    pub fn clear_token() {
        Self::TOKEN.with(|r| r.replace(None));
        LocalStorageToken::clear();
    }

    pub fn mutate_state<T: FnOnce(&mut State) -> State>(f: T) {
        Self::STATE.with(|r| r.replace_with(f));
        LocalStorageState::set(Self::STATE.with(|r| r.clone()));
    }
    pub fn get_state() -> Rc<RefCell<State>> {
        let from_static = Self::STATE.with(|r| r.clone());
        // Every time after the first, this synced should be true and the if body should not process.
        // This is because there is no way to unset synced once it is set on the static state.
        if !(*from_static).borrow().synced {
            // One in storage is always synced, we get from storage only once at the very first get
            Self::STATE.with(|r| r.replace(LocalStorageState::value()));
        }
        Self::STATE.with(|r| r.clone())
    }
    pub fn set_state(mut s: State) {
        s.synced = true;
        Self::STATE.with(|r| r.replace(s));
        LocalStorageState::set(Self::STATE.with(|r| r.clone()));
    }
}
