use std::borrow::Cow;

use log::debug;

use crate::{components::{entries, login}, state::State};

pub fn login_or_entries(url: &str, state: Cow<State>) {
    debug!("Router initialized 3 login or entries");
    debug!("{:?}", State::get_token());
    if State::get_token().is_none() {
        login(url, state);
    } else {
        entries(url, state);
    }
}
