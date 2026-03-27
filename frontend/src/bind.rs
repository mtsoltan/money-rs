use web_sys::*;
use log::error;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;
use gloo_utils::document;

// TODO(40): Use proper error propagation

thread_local! {
    static PARSER: std::cell::RefCell<DomParser> = std::cell::RefCell::new(DomParser::new().unwrap_throw());
}

pub fn log_on_err<V>(result: Result<V, JsValue>) -> Option<V> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            match e.as_string() {
                Some(s) => error!("{}", s),
                None => error!("{:?}", e),
            }
            None
        }
    }
}

pub fn init_wasm_logger() {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::default());
}

pub fn location() -> String {
    window().expect_throw("X021: Window").location().pathname().expect_throw("X021: Window")
}

pub fn history() -> web_sys::History {
    window().expect_throw("X021: Window").history().expect_throw("X021: Window")
}

pub fn get_element_by_id<T: JsCast>(element_id: &str) -> T {
    document().get_element_by_id(element_id).expect_throw(format!("X020: Element {element_id} not found").as_str()).unchecked_into::<T>()
}

pub fn root() -> HtmlDivElement {
    get_element_by_id("root")
}

pub fn page() -> HtmlDivElement {
    get_element_by_id("page")
}

pub fn form_data(e: SubmitEvent) -> FormData {
    let form = e.target().expect_throw("X013: SubmitEvent should have target").unchecked_into::<HtmlFormElement>();
    FormData::new_with_form(&form).expect_throw("X019: form data")
}

macro_rules! set_event {
    ($id:ident, $event:ty) => {
        pub fn $id<F>(e: &web_sys::HtmlElement, callback: F)
        where F: wasm_bindgen::closure::IntoWasmClosure<dyn FnMut($event)> + 'static {
            let closure = Closure::new(callback);
            e.$id(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
        }
    };
}

set_event!(set_onsubmit, SubmitEvent);
set_event!(set_onclick, MouseEvent);

#[derive(thiserror::Error, Debug)]
pub enum FormError {
    #[error("failed to get target from submit event")]
    NoEventTarget,
    #[error("failed to get form data from target with error: {0:?}")]
    NoFormData(JsValue),
    #[error("failed to cast form data into form struct: {0:?}")]
    FormDataCastError(String),
}

pub trait TryFromFormData: Sized {
    fn try_from(form: FormData) -> Result<Self, FormError>;
}
