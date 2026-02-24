use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlElement, HtmlInputElement, HtmlSelectElement, HtmlTableSectionElement};
use std::borrow::Cow;
use std::cell::RefCell;
use std::str::FromStr;

use fpdec::{Decimal, ParseDecimalError};
use html_escape::encode_text;
use wasm_bindgen::closure::Closure;
use crate::api::ApiClient;
use crate::components::empty_page;
use crate::components::page::{append_navbar, navbar_page};
use crate::router::Router;
use crate::state::State;

// TODO(40): Edit buttons and updates
use model::entity::{
    CategoryResponse, CurrencyResponse, SourceResponse,
    CreateCategoryRequest,
    CreateCurrencyRequest,
    CreateSourceRequest,
};
use model::numeric::Numeric;
use crate::components::lib::append;

// TODO(30): Outline inlined styles
// TODO(30): Clean up logic
thread_local! {
    static CATEGORIES: RefCell<Vec<CategoryResponse>> = RefCell::new(vec![]);
    static CURRENCIES: RefCell<Vec<CurrencyResponse>> = RefCell::new(vec![]);
    static SOURCES:    RefCell<Vec<SourceResponse>>    = RefCell::new(vec![]);
}

// TODO(1): Use state if present, and parse state from query parameters if first load
// TODO(30): Remember to set state and query parameters in a unified interface on state change, replace_with_state instead of push_with_state on half-transitions
pub fn setup(_: &str, state: Cow<State>) {
    navbar_page();
    let h = html_to_string_macro::html! {
        <h1>{"Setup"}</h1>
    };
    append(h.as_str());
}


/*

fn get_value(r: &NodeRef) -> String {
    r.cast::<HtmlInputElement>()
        .map(|i| i.value())
        .or_else(|| r.cast::<HtmlSelectElement>().map(|s| s.value()))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn set_text(el: &NodeRef, html: &str) {
    if let Some(d) = el.cast::<HtmlElement>() {
        d.set_inner_html(html);
    }
}

fn tbody_html(el: &NodeRef, html: &str) {
    if let Some(d) = el.cast::<HtmlTableSectionElement>() {
        d.set_inner_html(html);
    }
}

fn render_categories(into: &NodeRef, filter_name: &NodeRef) {
    let name_filter = get_value(filter_name).to_lowercase();
    CATEGORIES.with(|store| {
        let mut html = String::new();
        for c in store.borrow().iter() {
            if !name_filter.is_empty() && !c.name.to_lowercase().contains(&name_filter) { continue; }

            html.push_str(&format!(
                "<tr data-name='{name}'>
                    <td>{name}</td>
                    <td style='text-align:center;'>{archived}</td>
                    <td class='toolbar'>
                      <button class='button warn action-archive' data-name='{name}'>Archive</button>
                    </td>
                 </tr>",
                name = encode_text(&c.name),
                archived = if c.archived { "✓" } else { "" },
            ));
        }
        tbody_html(into, &html);
    });
}

fn sort_categories(by: &str, asc: bool, into: NodeRef, filter_name: NodeRef) {
    CATEGORIES.with(|store| {
        let mut v = store.borrow().clone();
        match by {
            "name" => {
                if asc { v.sort_by(|a,b| a.name.cmp(&b.name)); }
                else   { v.sort_by(|a,b| b.name.cmp(&a.name)); }
            }
            _ => {}
        }
        store.replace(v);
    });
    render_categories(&into, &filter_name);
}

fn wire_categories_actions(tbody: &NodeRef, reload_cb: Callback<()>) {
    if let Some(body) = tbody.cast::<HtmlTableSectionElement>() {
        let list = body.query_selector_all("button.action-archive").unwrap();
        for i in 0..list.length() {
            if let Some(node) = list.item(i) {
                if let Ok(button) = node.dyn_into::<web_sys::HtmlElement>() {
                    let name = button.get_attribute("data-name").unwrap_or_default();
                    let onclick = Closure::wrap(Box::new(|| {
                        let name = name.clone();
                        let reload_cb = reload_cb.clone();
                        spawn_local(async move {
                            let _ = ApiClient::archive_category(&name).await;
                            reload_cb.emit(());
                        });
                    }) as Box<dyn FnMut()>);
                    button.set_onclick(Some(onclick.as_ref().unchecked_ref()));
                    onclick.forget();
                }
            }
        }
    }
}

fn render_currencies(into: &NodeRef, filter_name: &NodeRef) {
    let name_filter = get_value(filter_name).to_lowercase();
    CURRENCIES.with(|store| {
        let mut html = String::new();
        for c in store.borrow().iter() {
            if !name_filter.is_empty() && !c.name.to_lowercase().contains(&name_filter) { continue; }

            html.push_str(&format!(
                "<tr data-name='{name}'>
                    <td>{name}</td>
                    <td style='text-align:right;' title='rate_to_fixed'>{rtf}</td>
                    <td style='text-align:center;'>{archived}</td>
                    <td class='toolbar'>
                      <button class='button warn action-archive' data-name='{name}'>Archive</button>
                    </td>
                 </tr>",
                name = encode_text(&c.name),
                rtf  = c.rate_to_fixed, // Decimal already Display
                archived = if c.archived { "✓" } else { "" },
            ));
        }
        tbody_html(into, &html);
    });
}

fn sort_currencies(by: &str, asc: bool, into: NodeRef, filter_name: NodeRef) {
    CURRENCIES.with(|store| {
        let mut v = store.borrow().clone();
        match by {
            "name" => {
                if asc { v.sort_by(|a,b| a.name.cmp(&b.name)); }
                else   { v.sort_by(|a,b| b.name.cmp(&a.name)); }
            }
            "rtf" => {
                if asc { v.sort_by(|a,b| a.rate_to_fixed.cmp(&b.rate_to_fixed)); }
                else   { v.sort_by(|a,b| b.rate_to_fixed.cmp(&a.rate_to_fixed)); }
            }
            _ => {}
        }
        store.replace(v);
    });
    render_currencies(&into, &filter_name);
}

fn wire_currencies_actions(tbody: &NodeRef, reload_cb: Callback<()>) {
    if let Some(body) = tbody.cast::<HtmlTableSectionElement>() {
        let list = body.query_selector_all("button.action-archive").unwrap();
        for i in 0..list.length() {
            if let Some(node) = list.item(i) {
                if let Ok(button) = node.dyn_into::<web_sys::HtmlElement>() {
                    let name = button.get_attribute("data-name").unwrap_or_default();
                    let onclick = Closure::wrap(Box::new(|| {
                        let reload_cb = reload_cb.clone();
                        let name = name.clone();
                        spawn_local(async move {
                            let _ = ApiClient::archive_currency(&name).await;
                            reload_cb.emit(());
                        });
                    }) as Box<dyn FnMut()>);
                    button.set_onclick(Some(onclick.as_ref().unchecked_ref()));
                    onclick.forget();
                }
            }
        }
    }
}

fn render_sources(into: &NodeRef, filter_name: &NodeRef, filter_currency: &NodeRef) {
    let name_filter = get_value(filter_name).to_lowercase();
    let cur_filter  = get_value(filter_currency);
    SOURCES.with(|store| {
        let mut html = String::new();
        for s in store.borrow().iter() {
            if !name_filter.is_empty() && !s.name.to_lowercase().contains(&name_filter) { continue; }
            if !cur_filter.is_empty() && s.currency != cur_filter { continue; }

            html.push_str(&format!(
                "<tr data-name='{name}'>
                    <td>{name}</td>
                    <td>{currency}</td>
                    <td style='text-align:right;'>{amount}</td>
                    <td style='text-align:center;'>{archived}</td>
                    <td class='toolbar'>
                      <button class='button warn action-archive' data-name='{name}'>Archive</button>
                    </td>
                 </tr>",
                name     = encode_text(&s.name),
                currency = encode_text(&s.currency),
                amount   = s.amount,
                archived = if s.archived { "✓" } else { "" },
            ));
        }
        tbody_html(into, &html);
    });
}

fn sort_sources(by: &str, asc: bool, into: NodeRef, filter_name: NodeRef, filter_currency: NodeRef) {
    SOURCES.with(|store| {
        let mut v = store.borrow().clone();
        match by {
            "name" => { if asc { v.sort_by(|a,b| a.name.cmp(&b.name)) } else { v.sort_by(|a,b| b.name.cmp(&a.name)) } }
            "amount" => { if asc { v.sort_by(|a,b| a.amount.cmp(&b.amount)) } else { v.sort_by(|a,b| b.amount.cmp(&a.amount)) } }
            "currency" => { if asc { v.sort_by(|a,b| a.currency.cmp(&b.currency)) } else { v.sort_by(|a,b| b.currency.cmp(&a.currency)) } }
            _ => {}
        }
        store.replace(v);
    });
    render_sources(&into, &filter_name, &filter_currency);
}

fn wire_sources_actions(tbody: &NodeRef, reload_cb: Callback<()>) {
    if let Some(body) = tbody.cast::<HtmlTableSectionElement>() {
        let list = body.query_selector_all("button.action-archive").unwrap();
        for i in 0..list.length() {
            if let Some(node) = list.item(i) {
                if let Ok(button) = node.dyn_into::<HtmlElement>() {
                    let name = button.get_attribute("data-name").unwrap_or_default();
                    let onclick = Closure::wrap(Box::new(|| {
                        let name = name.clone();
                        let reload_cb = reload_cb.clone();
                        spawn_local(async move {
                            let _ = ApiClient::archive_source(&name).await;
                            reload_cb.emit(());
                        });
                    }) as Box<dyn FnMut()>);
                    button.set_onclick(Some(onclick.as_ref().unchecked_ref()));
                    onclick.forget();
                }
            }
        }
    }
}

#[function_component(Setup)]
pub fn setup() -> Html {
    // feedback banner
    let fb = use_node_ref();

    // categories: nodes
    let cat_filter = use_node_ref();
    let cat_tbody  = use_node_ref();
    let cat_c_name = use_node_ref();

    // currencies: nodes
    let cur_filter = use_node_ref();
    let cur_tbody  = use_node_ref();
    let cur_c_name = use_node_ref();
    let cur_c_rtf  = use_node_ref();

    // sources: nodes
    let src_filter_name     = use_node_ref();
    let src_filter_currency = use_node_ref();
    let src_tbody           = use_node_ref();
    let src_c_name          = use_node_ref();
    let src_c_currency      = use_node_ref();
    let src_c_amount        = use_node_ref();

    let set_fb = {
        let fb = fb.clone();
        move |msg: &str, ok: bool| {
            set_text(&fb, &format!(
                "<div class='card' style='border-left:4px solid var({});padding:.5rem 1rem;margin-bottom:.75rem;'>{}</div>",
                if ok { "--ok" } else { "--danger" },
                encode_text(msg)
            ));
        }
    };

    // reloaders
    let reload_categories = {
        let cat_tbody = cat_tbody.clone();
        let cat_filter = cat_filter.clone();
        Callback::from(move |_| {
            let cat_tbody = cat_tbody.clone();
            let cat_filter = cat_filter.clone();
            spawn_local(async move {
                match ApiClient::get_categories().await {
                    Ok(v) => {
                        CATEGORIES.with(|s| s.replace(v));
                        render_categories(&cat_tbody.clone(), &cat_filter);
                        wire_categories_actions(&cat_tbody.clone(), Callback::from(move |_| {
                            // re-fetch after archive
                            let cat_tbody = cat_tbody.clone();
                            let cat_filter = cat_filter.clone();
                            spawn_local(async move {
                                if let Ok(v) = ApiClient::get_categories().await {
                                    CATEGORIES.with(|s| s.replace(v));
                                    render_categories(&cat_tbody, &cat_filter);
                                    wire_categories_actions(&cat_tbody, Callback::from(|_| ()));
                                }
                            });
                        }));
                    }
                    Err(e) => web_sys::window().unwrap().alert_with_message(&format!("get_categories: {e}")).ok().expect("X094: Shouldn't be none"),
                };
            });
        })
    };
    let reload_categories_clone = reload_categories.clone();
    let reload_categories_mouse = Callback::<MouseEvent>::from(move |_| reload_categories_clone.clone().emit(()));

    let reload_currencies = {
        let cur_tbody = cur_tbody.clone();
        let cur_filter = cur_filter.clone();
        Callback::from(move |_| {
            let cur_tbody = cur_tbody.clone();
            let cur_filter = cur_filter.clone();
            spawn_local(async move {
                match ApiClient::get_currencies().await {
                    Ok(v) => {
                        CURRENCIES.replace(v);
                        render_currencies(&cur_tbody.clone(), &cur_filter);
                        wire_currencies_actions(&cur_tbody.clone(), Callback::from(move |_| {
                            let cur_tbody = cur_tbody.clone();
                            let cur_filter = cur_filter.clone();
                            spawn_local(async move {
                                if let Ok(v) = ApiClient::get_currencies().await {
                                    CURRENCIES.replace(v);
                                    render_currencies(&cur_tbody, &cur_filter);
                                    wire_currencies_actions(&cur_tbody, Callback::from(|_| ()));
                                }
                            });
                        }));
                    }
                    Err(e) => web_sys::window().unwrap().alert_with_message(&format!("get_currencies: {e}")).ok().expect("X093: Shouldn't be none"),
                };
            });
        })
    };
    let reload_currencies_clone = reload_currencies.clone();
    let reload_currencies_mouse = Callback::<MouseEvent>::from(move |_| reload_currencies_clone.clone().emit(()));

    let reload_sources = {
        let src_tbody = src_tbody.clone();
        let src_filter_name = src_filter_name.clone();
        let src_filter_currency = src_filter_currency.clone();
        Callback::<()>::from(move |_| {
            let src_tbody = src_tbody.clone();
            let src_filter_name = src_filter_name.clone();
            let src_filter_currency = src_filter_currency.clone();
            spawn_local(async move {
                match ApiClient::get_sources().await {
                    Ok(v) => {
                        SOURCES.with(|s| s.replace(v));
                        render_sources(&src_tbody.clone(), &src_filter_name, &src_filter_currency);
                        wire_sources_actions(&src_tbody.clone(), Callback::from(move |_| {
                            let src_tbody = src_tbody.clone();
                            let src_filter_name = src_filter_name.clone();
                            let src_filter_currency = src_filter_currency.clone();
                            spawn_local(async move {
                                if let Ok(v) = ApiClient::get_sources().await {
                                    SOURCES.with(|s| s.replace(v));
                                    render_sources(&src_tbody, &src_filter_name, &src_filter_currency);
                                    wire_sources_actions(&src_tbody, Callback::from(|_| ()));
                                }
                            });
                        }));
                    }
                    Err(e) => web_sys::window().unwrap().alert_with_message(&format!("get_sources: {e}")).ok().expect("X092: Shouldn't be none"),
                };
            });
        })
    };
    let reload_sources_clone = reload_sources.clone();
    let reload_sources_mouse = Callback::<MouseEvent>::from(move |_| reload_sources_clone.emit(()));

    // filter events
    let cat_filter_oninput = {
        let cat_tbody = cat_tbody.clone();
        let cat_filter = cat_filter.clone();
        Callback::<InputEvent>::from(move |_| render_categories(&cat_tbody, &cat_filter))
    };
    let cur_filter_oninput = {
        let cur_tbody = cur_tbody.clone();
        let cur_filter = cur_filter.clone();
        Callback::<InputEvent>::from(move |_| render_currencies(&cur_tbody, &cur_filter))
    };
    let src_filter_oninput = {
        let src_tbody = src_tbody.clone();
        let src_filter_name = src_filter_name.clone();
        let src_filter_currency = src_filter_currency.clone();
        Callback::<InputEvent>::from(move |_| render_sources(&src_tbody, &src_filter_name, &src_filter_currency))
    };
    let src_filter_currency_onchange = {
        let src_tbody = src_tbody.clone();
        let src_filter_name = src_filter_name.clone();
        let src_filter_currency = src_filter_currency.clone();
        Callback::<Event>::from(move |_| render_sources(&src_tbody, &src_filter_name, &src_filter_currency))
    };

    // create handlers
    let create_category = {
        let cat_c_name = cat_c_name.clone();
        let reload_categories = reload_categories.clone();
        let set_fb = set_fb.clone();
        Callback::from(move |_| {
            let cat_c_name = cat_c_name.clone();
            let reload = reload_categories.clone();
            let set_fb = set_fb.clone();
            let name = get_value(&cat_c_name);
            if name.is_empty() { set_fb("Category name required", false); return; }
            let req = CreateCategoryRequest { name: name.clone(), archived: None };
            spawn_local(async move {
                match ApiClient::create_category(&req).await {
                    Ok(_) => { set_fb("Category created", true); reload.emit(()); }
                    Err(e) => set_fb(&format!("Create category failed: {e}"), false),
                }
            });
        })
    };
    let create_currency = {
        let cur_c_name = cur_c_name.clone();
        let cur_c_rtf = cur_c_rtf.clone();
        let set_fb = set_fb.clone();
        let reload_currencies = reload_currencies.clone();
        Callback::from(move |_| {
            let cur_c_name = cur_c_name.clone();
            let cur_c_rtf = cur_c_rtf.clone();
            let set_fb = set_fb.clone();
            let reload = reload_currencies.clone();
            let name = get_value(&cur_c_name);
            let rtf_s = get_value(&cur_c_rtf);
            if name.is_empty() { set_fb("Currency name required", false); return; }
            if rtf_s.is_empty() { set_fb("rate_to_fixed required", false); return; }
            let rtf = match Decimal::from_str(&rtf_s) {
                Ok(d) => d, Err(_) => { set_fb("rate_to_fixed invalid", false); return; }
            };
            let req = CreateCurrencyRequest { name: name.clone(), rate_to_fixed: rtf };
            spawn_local(async move {
                match ApiClient::create_currency(&req).await {
                    Ok(_) => { set_fb("Currency created", true); reload.emit(()); }
                    Err(e) => set_fb(&format!("Create currency failed: {e}"), false),
                }
            });
        })
    };
    let create_source = {
        let src_c_name = src_c_name.clone();
        let src_c_currency = src_c_currency.clone();
        let src_c_amount = src_c_amount.clone();
        let set_fb = set_fb.clone();
        let reload_sources = reload_sources.clone();
        Callback::from(move |_| {
            let src_c_name = src_c_name.clone();
            let src_c_currency = src_c_currency.clone();
            let src_c_amount = src_c_amount.clone();
            let set_fb = set_fb.clone();
            let reload = reload_sources.clone();
            let name = get_value(&src_c_name);
            let currency = get_value(&src_c_currency);
            let amount_s = get_value(&src_c_amount);
            if name.is_empty() { set_fb("Source name required", false); return; }
            if currency.is_empty() { set_fb("Source currency required", false); return; }
            let amount = if amount_s.is_empty() { None } else {
                match Decimal::from_str(amount_s.as_str()) {
                    Ok(d) => Some(d),
                    Err(_) => {
                        set_fb("amount invalid", false);
                        return;
                    }
                }
            };
            let req = CreateSourceRequest { name: name.clone(), currency, amount, archived: None };
            spawn_local(async move {
                match ApiClient::create_source(&req).await {
                    Ok(_) => { set_fb("Source created", true); reload.emit(()); }
                    Err(e) => set_fb(&format!("Create source failed: {e}"), false),
                }
            });
        })
    };

    html! {
      <>
        <div ref={fb}></div>

        /* -------------------- Categories -------------------- */
        <div class="card" style="padding:0;margin-bottom:1rem;">
          <div class="toolbar" style="padding:.6rem 1rem;border-bottom:1px solid var(--border);">
            <div class="toolbar">
              <button class="button" onclick={reload_categories_mouse.clone()}>{ "Load categories" }</button>
              <button class="button" onclick={{
                let cat_tbody = cat_tbody.clone(); let cat_filter = cat_filter.clone();
                Callback::from(move |_| sort_categories("name", true, cat_tbody.clone(), cat_filter.clone()))
              }}>{ "Sort name↑" }</button>
              <button class="button" onclick={{
                let cat_tbody = cat_tbody.clone(); let cat_filter = cat_filter.clone();
                Callback::from(move |_| sort_categories("name", false, cat_tbody.clone(), cat_filter.clone()))
              }}>{ "Sort name↓" }</button>
            </div>
            <div class="toolbar">
              <input class="input" type="text" placeholder="filter: name" ref={cat_filter.clone()} oninput={cat_filter_oninput}/>
            </div>
          </div>

          <table class="table">
            <thead>
              <tr>
                <th>{ "Name" }</th>
                <th style="text-align:center;">{ "Archived" }</th>
                <th>{ "Actions" }</th>
              </tr>
            </thead>
            <tbody ref={cat_tbody}></tbody>
            <tfoot>
              <tr>
                <td><input class="input" type="text" placeholder="name" ref={cat_c_name}/></td>
                <td></td>
                <td><button class="button ok" onclick={create_category}>{ "Create" }</button></td>
              </tr>
            </tfoot>
          </table>
        </div>

        /* -------------------- Currencies -------------------- */
        <div class="card" style="padding:0;margin-bottom:1rem;">
          <div class="toolbar" style="padding:.6rem 1rem;border-bottom:1px solid var(--border);">
            <div class="toolbar">
              <button class="button" onclick={reload_currencies_mouse.clone()}>{ "Load currencies" }</button>
              <button class="button" onclick={{
                let cur_tbody = cur_tbody.clone(); let cur_filter = cur_filter.clone();
                Callback::from(move |_| sort_currencies("name", true, cur_tbody.clone(), cur_filter.clone()))
              }}>{ "Sort name↑" }</button>
              <button class="button" onclick={{
                let cur_tbody = cur_tbody.clone(); let cur_filter = cur_filter.clone();
                Callback::from(move |_| sort_currencies("name", false, cur_tbody.clone(), cur_filter.clone()))
              }}>{ "Sort name↓" }</button>
              <button class="button" onclick={{
                let cur_tbody = cur_tbody.clone(); let cur_filter = cur_filter.clone();
                Callback::from(move |_| sort_currencies("rtf", true, cur_tbody.clone(), cur_filter.clone()))
              }}>{ "Sort rate↑" }</button>
              <button class="button" onclick={{
                let cur_tbody = cur_tbody.clone(); let cur_filter = cur_filter.clone();
                Callback::from(move |_| sort_currencies("rtf", false, cur_tbody.clone(), cur_filter.clone()))
              }}>{ "Sort rate↓" }</button>
            </div>
            <div class="toolbar">
              <input class="input" type="text" placeholder="filter: name" ref={cur_filter.clone()} oninput={cur_filter_oninput}/>
            </div>
          </div>

          <table class="table">
            <thead>
              <tr>
                <th>{ "Name" }</th>
                <th style="text-align:right;">{ "Rate to fixed" }</th>
                <th style="text-align:center;">{ "Archived" }</th>
                <th>{ "Actions" }</th>
              </tr>
            </thead>
            <tbody ref={cur_tbody}></tbody>
            <tfoot>
              <tr>
                <td><input class="input" type="text" placeholder="name" ref={cur_c_name}/></td>
                <td><input class="input" type="text" placeholder="rate_to_fixed" ref={cur_c_rtf}/></td>
                <td></td>
                <td><button class="button ok" onclick={create_currency}>{ "Create" }</button></td>
              </tr>
            </tfoot>
          </table>
        </div>

        /* -------------------- Sources -------------------- */
        <div class="card" style="padding:0;">
          <div class="toolbar" style="padding:.6rem 1rem;border-bottom:1px solid var(--border);">
            <div class="toolbar">
              <button class="button" onclick={reload_sources_mouse.clone()}>{ "Load sources" }</button>
              <button class="button" onclick={{
                let src_tbody = src_tbody.clone(); let src_filter_name = src_filter_name.clone(); let src_filter_currency = src_filter_currency.clone();
                Callback::from(move |_| sort_sources("name", true, src_tbody.clone(), src_filter_name.clone(), src_filter_currency.clone()))
              }}>{ "Sort name↑" }</button>
              <button class="button" onclick={{
                let src_tbody = src_tbody.clone(); let src_filter_name = src_filter_name.clone(); let src_filter_currency = src_filter_currency.clone();
                Callback::from(move |_| sort_sources("name", false, src_tbody.clone(), src_filter_name.clone(), src_filter_currency.clone()))
              }}>{ "Sort name↓" }</button>
              <button class="button" onclick={{
                let src_tbody = src_tbody.clone(); let src_filter_name = src_filter_name.clone(); let src_filter_currency = src_filter_currency.clone();
                Callback::from(move |_| sort_sources("amount", true, src_tbody.clone(), src_filter_name.clone(), src_filter_currency.clone()))
              }}>{ "Sort amount↑" }</button>
              <button class="button" onclick={{
                let src_tbody = src_tbody.clone(); let src_filter_name = src_filter_name.clone(); let src_filter_currency = src_filter_currency.clone();
                Callback::from(move |_| sort_sources("amount", false, src_tbody.clone(), src_filter_name.clone(), src_filter_currency.clone()))
              }}>{ "Sort amount↓" }</button>
            </div>
            <div class="toolbar">
              <input class="input" type="text" placeholder="filter: name" ref={src_filter_name.clone()} oninput={src_filter_oninput.clone()}/>
              <input class="input" type="text" placeholder="filter: currency" ref={src_filter_currency.clone()} oninput={src_filter_oninput}/>
            </div>
          </div>

          <table class="table">
            <thead>
              <tr>
                <th>{ "Name" }</th>
                <th>{ "Currency" }</th>
                <th style="text-align:right;">{ "Amount" }</th>
                <th style="text-align:center;">{ "Archived" }</th>
                <th>{ "Actions" }</th>
              </tr>
            </thead>
            <tbody ref={src_tbody}></tbody>
            <tfoot>
              <tr>
                <td><input class="input" type="text" placeholder="name" ref={src_c_name}/></td>
                <td><input class="input" type="text" placeholder="currency" ref={src_c_currency}/></td>
                <td><input class="input" type="text" placeholder="amount (opt)" ref={src_c_amount}/></td>
                <td></td>
                <td><button class="button ok" onclick={create_source}>{ "Create" }</button></td>
              </tr>
            </tfoot>
          </table>
        </div>
      </>
    }
}

*/