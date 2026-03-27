use std::borrow::Cow;

use gloo_history::{BrowserHistory, History};
use html_to_string_macro::html;
use log::{debug, error};
use web_sys::{FormData, HtmlButtonElement, HtmlDivElement, HtmlFormElement, HtmlInputElement};
use model::entity::LoginRequest;
use crate::api::ApiClient;
use crate::bind;
use crate::bind::{FormError, TryFromFormData};
use crate::components::empty_page;
use crate::components::lib::{Route, append};
use crate::state::State;

impl TryFromFormData for LoginRequest {
    fn try_from(form: FormData) -> Result<Self, FormError> {
        let Some(username) = form.get("username").as_string() else {
            return Err(FormError::FormDataCastError("username must be a string".to_owned()));
        };
        let Some(password) = form.get("password").as_string() else {
            return Err(FormError::FormDataCastError("password must be a string".to_owned()));
        };
        Ok(LoginRequest {
            username,
            password,
        })
    }
}

pub fn login(_: &str, _: Cow<State>) {
    empty_page();
    debug!("Router initialized 3 login");
    let h = html! {
        <div class="container narrow-form">
            <h2>{ "Login" }</h2>
            <div class="card">
                <form class="stack" id="login-form">
                    <input name="username" id="username-field" class="input" type="text" placeholder="Username" />
                    <input name="password" id="password-field" class="input" type="password" placeholder="Password" />
                    <div id="error-div" class="error"></div>
                    <div class="toolbar">
                        <button id="login-button" class="button primary" type="submit">{ "Sign in" }</button>
                    </div>
                </form>
            </div>
        </div>
    };
    append(h.as_str());
    let login_form = bind::get_element_by_id::<HtmlFormElement>("login-form");
    let error_div = bind::get_element_by_id::<HtmlDivElement>("error-div");
    debug!("aaaa");

    bind::set_onsubmit(&login_form, move |e: web_sys::SubmitEvent| {
        e.prevent_default();
        error_div.set_text_content(None);
        let form_data = bind::form_data(e);

        let error_div = error_div.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let login_button = bind::get_element_by_id::<HtmlButtonElement>("login-button");
            let username_field = bind::get_element_by_id::<HtmlInputElement>("username-field");
            let password_field = bind::get_element_by_id::<HtmlInputElement>("password-field");
            let req = match <LoginRequest as TryFromFormData>::try_from(form_data) {
                Ok(req) => {
                    req
                },
                Err(e) => {
                    error!("Failed to parse username or password: {}", e);
                    return;
                }
            };
            login_button.set_disabled(true);
            login_button.set_text_content(Some("Signing in..."));
            username_field.set_disabled(true);
            password_field.set_disabled(true);

            match ApiClient::login(req).await {
                Ok(_) => {
                    // TODO(1): Modify router to handle authentication middleware (support middleware and not just hooks)
                    // TODO(1): Test soft redirect using visit
                    Route::Entries.visit();
                }
                Err(e) => {
                    login_button.set_disabled(false);
                    username_field.set_disabled(true);
                    password_field.set_disabled(true);
                    login_button.set_text_content(Some("Sign in"));
                    error_div.set_text_content(Some(&format!("{}", e)));
                }
            }
        });
    });
}
