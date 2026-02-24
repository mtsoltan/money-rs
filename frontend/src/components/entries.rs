use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, HtmlSelectElement, HtmlButtonElement, HtmlTableSectionElement, HtmlTableElement, HtmlElement, js_sys};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use fpdec::Decimal;
use crate::api::ApiClient;
use crate::components::page::{append_navbar, navbar_page};
use crate::date::{js_now_to_naive_date, preset_range, Preset};
use crate::router::Router;
use crate::state::State;
use wasm_bindgen_futures::spawn_local;
use model::entity::{EntryResponse, FindEntriesResponse, CreateEntryRequest, EntryType, EntryQuery};
use crate::components::lib::append;
// TODO(30): Outline inlined styles
// TODO(30): Clean up logic

// Global in-memory model. Updated only from user events.
thread_local! {
    static ENTRIES: RefCell<Vec<EntryResponse>> = RefCell::new(Vec::new());
    static SELECTED: RefCell<HashSet<i32>> = RefCell::new(HashSet::new());
    static LAST_FIND_META: RefCell<Option<FindMeta>> = RefCell::new(None);
}

#[derive(Clone)]
struct FindMeta {
    after: Option<String>,
    before: Option<String>,
    server_side: bool,
    sort: Option<String>,
}

// TODO(1): Use state if present, and parse state from query parameters if first load
// TODO(30): Remember to set state and query parameters in a unified interface on state change, replace_with_state instead of push_with_state on half-transitions
pub fn entries(_: &str, state: Cow<State>) {
    navbar_page();
    let h = html_to_string_macro::html! {
        <h1>{"Entries"}</h1>
    };
    append(h.as_str());
}

/*
#[function_component(EntriesTable)]
pub fn entries_table() -> Html {
    // --- Refs to imperative DOM targets ---
    let stats_ref = use_node_ref();      // <div> above table (stats)
    let table_ref = use_node_ref();      // <table> for header hookups
    let tbody_ref = use_node_ref();      // <tbody> filled with rows

    // Header controls (search row)
    let q_desc_ref = use_node_ref();
    let q_currency_ref = use_node_ref();
    let q_source_ref = use_node_ref();
    let q_secondary_ref = use_node_ref();
    let q_category_ref = use_node_ref();
    let q_type_ref = use_node_ref();
    let q_amount_eq_ref = use_node_ref();
    let q_amount_min_ref = use_node_ref();
    let q_amount_max_ref = use_node_ref();
    let q_date_eq_ref = use_node_ref();
    let q_date_after_ref = use_node_ref();
    let q_date_before_ref = use_node_ref();
    let q_preset_ref = use_node_ref();
    let server_toggle_ref = use_node_ref();

    // Filters row (client-side filters)
    let f_desc_ref = use_node_ref();
    let f_currency_ref = use_node_ref();
    let f_source_ref = use_node_ref();
    let f_secondary_ref = use_node_ref();
    let f_category_ref = use_node_ref();
    let f_type_ref = use_node_ref();
    let f_amount_eq_ref = use_node_ref();
    let f_amount_min_ref = use_node_ref();
    let f_amount_max_ref = use_node_ref();
    let f_date_eq_ref = use_node_ref();
    let f_date_after_ref = use_node_ref();
    let f_date_before_ref = use_node_ref();

    // Footer (create / bulk)
    let create_button_ref = use_node_ref();
    let c_desc_ref = use_node_ref();
    let c_long_ref = use_node_ref();
    let c_target_ref = use_node_ref();
    let c_category_ref = use_node_ref();
    let c_amount_ref = use_node_ref();
    let c_currency_ref = use_node_ref();
    let c_type_ref = use_node_ref();
    let c_source_ref = use_node_ref();
    let c_source_amount_ref = use_node_ref();
    let c_secondary_ref = use_node_ref();
    let c_secondary_amount_ref = use_node_ref();
    let c_date_ref = use_node_ref();

    // --- Helpers ---
    let get_value = |r: &NodeRef| -> String {
        r.cast::<HtmlInputElement>()
            .map(|i| i.value())
            .or_else(|| r.cast::<HtmlSelectElement>().map(|s| s.value()))
            .unwrap_or_default()
            .trim()
            .to_string()
    };

    let get_checked = |r: &NodeRef| -> bool {
        r.cast::<HtmlInputElement>().map(|i| i.checked()).unwrap_or(false)
    };

    // --- Rendering helpers (imperative DOM) ---
    let render_stats = {
        let stats_ref = stats_ref.clone();
        Callback::from(move |resp: FindEntriesResponse| {
            if let Some(div) = stats_ref.cast::<HtmlElement>() {
                let monthly_avg = resp.monthly_average.to_string();
                // sum_per_month as a sorted string
                let mut months: Vec<_> = resp.sum_per_month.iter().collect();
                months.sort_by_key(|(k, _)| (*k).clone());
                let month_lines: String = months.into_iter()
                    .map(|(k, v)| format!("<div><strong>{}</strong>: {}</div>", k, v))
                    .collect::<Vec<_>>()
                    .join("");
                div.set_inner_html(&format!(
                    "<div class='card' style='margin-bottom:.75rem;'>
                        <div class='toolbar'>
                          <div><strong>Average / month:</strong> {}</div>
                        </div>
                        <div style='margin-top:.5rem;display:grid;grid-template-columns:repeat(4,1fr);gap:.5rem;'>{}</div>
                    </div>",
                    monthly_avg,
                    month_lines
                ));
            }
        })
    };

    let render_rows = {
        let tbody_ref = tbody_ref.clone();
        Callback::from(move |entries: Vec<EntryResponse>| {
            if let Some(tbody) = tbody_ref.cast::<HtmlTableSectionElement>() {
                let mut html = String::new();
                for e in entries {
                    let id = e.id;
                    let sec_src = e.secondary_source.clone().unwrap_or_default();
                    let sec_amt = e.secondary_source_amount
                        .as_ref()
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "".into());
                    let sel = SELECTED.with(|s| s.borrow().contains(&id));
                    let row = format!(
                        "<tr data-id='{id}' class='entry-row'>
                            <td><input type='checkbox' class='sel' {checked}/></td>
                            <td>{date}</td>
                            <td title='{ldesc}'>{desc}</td>
                            <td>{category}</td>
                            <td>{currency}</td>
                            <td style='text-align:right;'>{amount}</td>
                            <td style='text-align:right;' title='{aif_hint}'>{amount_in_fixed}</td>
                            <td>{etype}</td>
                            <td>{source}</td>
                            <td>{sec_src}</td>
                            <td style='text-align:right;'>{sec_amt}</td>
                            <td>{created}</td>
                            <td>{archived}</td>
                            <td>
                              <button class='button warn action-archive'>Archive</button>
                              <button class='button danger action-delete'>Delete</button>
                            </td>
                         </tr>",
                        id = id,
                        checked = if sel { "checked" } else { "" },
                        date = e.date,
                        ldesc = html_escape::encode_text(&e.long_description.clone().unwrap_or_default()),
                        desc = html_escape::encode_text(&e.description),
                        category = html_escape::encode_text(&e.category),
                        currency = html_escape::encode_text(&e.currency),
                        amount = e.amount,
                        amount_in_fixed = e.amount_in_fixed,
                        aif_hint = format!("conv_to_fixed={}", e.conversion_rate_to_fixed),
                        etype = format!("{:?}", e.entry_type),
                        source = html_escape::encode_text(&e.source),
                        sec_src = html_escape::encode_text(&sec_src),
                        sec_amt = sec_amt,
                        created = e.created_at,
                        archived = if e.archived { "✓" } else { "" },
                    );
                    html.push_str(&row);
                }
                tbody.set_inner_html(&html);
            }
        })
    };

    let recompute_selection_stats = {
        let stats_ref = stats_ref.clone();
        Callback::from(move |_| {
            let (sum, count) = ENTRIES.with(|es| {
                let es = es.borrow();
                SELECTED.with(|sel| {
                    let sel = sel.borrow();
                    if sel.is_empty() {
                        let s: Decimal = es.iter().map(|e| e.amount).reduce(|acc, e| acc + e).expect("X099: This sum should always be valid");
                        (s, es.len())
                    } else {
                        let mut s = Decimal::ZERO;
                        let mut c = 0usize;
                        for e in es.iter() {
                            if sel.contains(&e.id) {
                                s += e.amount.clone();
                                c += 1;
                            }
                        }
                        (s, c)
                    }
                })
            });
            if let Some(div) = stats_ref.cast::<HtmlElement>() {
                div.insert_adjacent_html("afterbegin", &format!(
                    "<div class='card' style='margin-bottom:.75rem;'>
                        <div><strong>Selection sum:</strong> {} &nbsp; <strong>count:</strong> {}</div>
                    </div>", sum, count
                )).ok();
            }
        })
    };

    // --- Client-side filter application ---
    fn apply_filters(entries: &[EntryResponse], refs: &ClientFilterRefs) -> Vec<EntryResponse> {
        let s = |r: &NodeRef| r.cast::<HtmlInputElement>().map(|i| i.value()).unwrap_or_default().trim().to_string();
        let ss = |r: &NodeRef| r.cast::<HtmlSelectElement>().map(|i| i.value()).unwrap_or_default().trim().to_string();

        let desc = s(&refs.desc);
        let currency = ss(&refs.currency);
        let source = s(&refs.source);
        let secondary = s(&refs.secondary);
        let category = s(&refs.category);
        let etype = ss(&refs.etype);

        let a_eq = s(&refs.amount_eq);
        let a_min = s(&refs.amount_min);
        let a_max = s(&refs.amount_max);

        let d_eq = s(&refs.date_eq);
        let d_after = s(&refs.date_after);
        let d_before = s(&refs.date_before);

        entries.iter().filter(|e| {
            if !desc.is_empty() && !e.description.to_lowercase().contains(&desc.to_lowercase()) { return false; }
            if !currency.is_empty() && e.currency != currency { return false; }
            if !source.is_empty() && e.source != source { return false; }
            if !secondary.is_empty() && e.secondary_source.as_deref().unwrap_or("") != secondary { return false; }
            if !category.is_empty() && e.category != category { return false; }
            if !etype.is_empty() && format!("{:?}", e.entry_type) != etype { return false; }

            if !a_eq.is_empty() && e.amount.to_string() != a_eq { return false; }
            if !a_min.is_empty() {
                if let Ok(v) = Decimal::from_str(&a_min) {
                    if e.amount < v { return false; }
                }
            }
            if !a_max.is_empty() {
                if let Ok(v) = Decimal::from_str(&a_max) {
                    if e.amount > v { return false; }
                }
            }

            if !d_eq.is_empty() && e.date != d_eq { return false; }
            if !d_after.is_empty() && e.date <= d_after { return false; }
            if !d_before.is_empty() && e.date >= d_before { return false; }

            true
        }).cloned().collect()
    }

    struct ClientFilterRefs {
        desc: NodeRef, currency: NodeRef, source: NodeRef, secondary: NodeRef,
        category: NodeRef, etype: NodeRef,
        amount_eq: NodeRef, amount_min: NodeRef, amount_max: NodeRef,
        date_eq: NodeRef, date_after: NodeRef, date_before: NodeRef,
    }

    let filter_apply = {
        let tbody_ref = tbody_ref.clone();
        let render_rows = render_rows.clone();
        let filter_refs = ClientFilterRefs {
            desc: f_desc_ref.clone(), currency: f_currency_ref.clone(), source: f_source_ref.clone(),
            secondary: f_secondary_ref.clone(), category: f_category_ref.clone(), etype: f_type_ref.clone(),
            amount_eq: f_amount_eq_ref.clone(), amount_min: f_amount_min_ref.clone(), amount_max: f_amount_max_ref.clone(),
            date_eq: f_date_eq_ref.clone(), date_after: f_date_after_ref.clone(), date_before: f_date_before_ref.clone(),
        };
        Callback::from(move |_| {
            ENTRIES.with(|store| {
                let filtered = apply_filters(&store.borrow(), &filter_refs);
                render_rows.emit(filtered);
            });
        })
    };

    let filter_apply_event = {
        let tbody_ref = tbody_ref.clone();
        let render_rows = render_rows.clone();
        let filter_refs = ClientFilterRefs {
            desc: f_desc_ref.clone(), currency: f_currency_ref.clone(), source: f_source_ref.clone(),
            secondary: f_secondary_ref.clone(), category: f_category_ref.clone(), etype: f_type_ref.clone(),
            amount_eq: f_amount_eq_ref.clone(), amount_min: f_amount_min_ref.clone(), amount_max: f_amount_max_ref.clone(),
            date_eq: f_date_eq_ref.clone(), date_after: f_date_after_ref.clone(), date_before: f_date_before_ref.clone(),
        };
        Callback::<Event>::from(move |_| {
            ENTRIES.with(|store| {
                let filtered = apply_filters(&store.borrow(), &filter_refs);
                render_rows.emit(filtered);
            });
        })
    };

    // --- Header sorting (amount/date: server if server_toggle checked, else client) ---
    let sort_handler = {
        let server_toggle_ref = server_toggle_ref.clone();
        let q_desc_ref = q_desc_ref.clone(); // we’ll reuse the last query when re-fetching
        let render_rows = render_rows.clone();
        let stats_ref = stats_ref.clone();
        Callback::from(move |sort_key: &'static str| {
            let server = server_toggle_ref.cast::<HtmlInputElement>().map(|i| i.checked()).unwrap_or(false);
            if server {
                // Rebuild a query from the current search row to preserve filters server-side
                let mut q = EntryQuery::default();
                let desc = q_desc_ref.cast::<HtmlInputElement>().map(|i| i.value()).unwrap_or_default();
                if !desc.trim().is_empty() { q.description = Some(desc.trim().into()); }
                q.sort = Some(sort_key.to_string());

                spawn_local({
                    let render_rows = render_rows.clone();
                    let stats_ref = stats_ref.clone();
                    async move {
                        match ApiClient::find_entries(&q).await {
                            Ok(resp) => {
                                ENTRIES.with(|s| s.replace(resp.entries.clone()));
                                render_rows.emit(ENTRIES.with(|s| s.borrow().clone()));
                                // Lightweight stats render
                                if let Some(div) = stats_ref.cast::<HtmlElement>() {
                                    div.insert_adjacent_html("afterbegin", "<div class='card' style='margin-bottom:.5rem;'>Server-side sorted.</div>").ok();
                                }
                            }
                            Err(e) => {
                                web_sys::window().unwrap().alert_with_message(&format!("sort error: {e}")).ok();
                            }
                        }
                    }
                });
            } else {
                ENTRIES.with(|s| {
                    let mut v = s.borrow().clone();
                    match sort_key {
                        "amount_asc" => v.sort_by(|a,b| a.amount.cmp(&b.amount)),
                        "amount_desc" => v.sort_by(|a,b| b.amount.cmp(&a.amount)),
                        "date_asc" => v.sort_by(|a,b| a.date.cmp(&b.date)),
                        "date_desc" => v.sort_by(|a,b| b.date.cmp(&a.date)),
                        _ => {}
                    }
                    s.replace(v.clone());
                    render_rows.emit(v);
                });
            }
        })
    };

    // --- Search (server-side) ---
    let search_click = {
        let q_desc_ref = q_desc_ref.clone();
        let q_currency_ref = q_currency_ref.clone();
        let q_source_ref = q_source_ref.clone();
        let q_secondary_ref = q_secondary_ref.clone();
        let q_category_ref = q_category_ref.clone();
        let q_type_ref = q_type_ref.clone();
        let q_amount_eq_ref = q_amount_eq_ref.clone();
        let q_amount_min_ref = q_amount_min_ref.clone();
        let q_amount_max_ref = q_amount_max_ref.clone();
        let q_date_eq_ref = q_date_eq_ref.clone();
        let q_date_after_ref = q_date_after_ref.clone();
        let q_date_before_ref = q_date_before_ref.clone();
        let q_preset_ref = q_preset_ref.clone();
        let server_toggle_ref = server_toggle_ref.clone();

        Callback::from(move |_| {
            let render_rows = render_rows.clone();
            let render_stats = render_stats.clone();

            let mut q = EntryQuery::default();

            let v = |r: &NodeRef| -> String {
                r.cast::<HtmlInputElement>()
                    .map(|i| i.value())
                    .or_else(|| r.cast::<HtmlSelectElement>().map(|s| s.value()))
                    .unwrap_or_default().trim().to_string()
            };
            macro_rules! set {
                ($field:ident, $val:expr) => {{
                    let s = $val;
                    if !s.is_empty() { q.$field = Some(s); }
                }}
            }

            set!(description, v(&q_desc_ref));
            set!(currency, v(&q_currency_ref));
            let src = v(&q_source_ref);
            if !src.is_empty() { q.sources = Some(vec![src]); }
            // Secondary source filter is client-only; server only has primary/secondary query via endpoints.
            set!(categories, {
                let s = v(&q_category_ref);
                if s.is_empty() { Vec::new() } else { vec![s.clone()] }
            });
            let et = v(&q_type_ref);
            if !et.is_empty() {
                q.entry_types = Some(vec![match et.as_str() {
                    "Spend" => EntryType::Spend,
                    "Income" => EntryType::Income,
                    "Lend" => EntryType::Lend,
                    "Borrow" => EntryType::Borrow,
                    "Convert" => EntryType::Convert,
                    _ => EntryType::Spend,
                }]);
            }

            let ae = v(&q_amount_eq_ref); if !ae.is_empty() { q.amount = Decimal::from_str(&ae).ok(); }
            let amin = v(&q_amount_min_ref); if !amin.is_empty() { q.min_amount = Decimal::from_str(&amin).ok(); }
            let amax = v(&q_amount_max_ref); if !amax.is_empty() { q.max_amount = Decimal::from_str(&amax).ok(); }

            let deq = v(&q_date_eq_ref); if !deq.is_empty() { q.date = Some(deq); }
            // Preset takes precedence over raw after/before if chosen
            let preset = v(&q_preset_ref);
            if !preset.is_empty() {
                // Use our util for last 3 months by default
                let dr = preset_range(Preset::Last3MonthsToNow, js_now_to_naive_date());
                q.after = Some(dr.after);
                q.before = Some(dr.before);
            } else {
                let da = v(&q_date_after_ref); if !da.is_empty() { q.after = Some(da); }
                let db = v(&q_date_before_ref); if !db.is_empty() { q.before = Some(db); }
            }

            let server_side = server_toggle_ref.cast::<HtmlInputElement>().map(|i| i.checked()).unwrap_or(false);
            if server_side {
                q.sort = Some("date_asc".to_string());
            }

            spawn_local(async move {
                match ApiClient::find_entries(&q).await {
                    Ok(resp) => {
                        ENTRIES.with(|store| store.replace(resp.entries.clone()));
                        render_rows.emit(resp.entries.clone());
                        render_stats.emit(resp);
                        LAST_FIND_META.with(|m| m.replace(Some(FindMeta {
                            after: q.after.clone(), before: q.before.clone(), server_side, sort: q.sort.clone()
                        })));
                    }
                    Err(e) => {
                        web_sys::window().unwrap().alert_with_message(&format!("search error: {e}")).ok();
                    }
                }
            });
        })
    };

    // Select all checkbox in header
    let select_all_click = {
        let tbody_ref = tbody_ref.clone();
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            let checked = {
                if let Some(t) = e.target_dyn_into::<HtmlInputElement>() {
                    t.checked()
                } else { false }
            };
            if let Some(tbody) = tbody_ref.cast::<HtmlTableSectionElement>() {
                let rows = tbody.query_selector_all("tr.entry-row").unwrap();
                SELECTED.with(|s| {
                    let mut set = s.borrow_mut();
                    for i in 0..rows.length() {
                        if let Some(row) = rows.item(i) {
                            let el = row.dyn_into::<web_sys::HtmlElement>().unwrap();
                            if let Ok(id) = el.get_attribute("data-id").unwrap_or_default().parse::<i32>() {
                                if checked { set.insert(id); } else { set.remove(&id); }
                            }
                            // Also tick the checkbox UI
                            if let Ok(cb) = el.query_selector("input.sel") {
                                if let Some(cb) = cb.and_then(|n| n.dyn_into::<HtmlInputElement>().ok()) {
                                    cb.set_checked(checked);
                                }
                            }
                        }
                    }
                });
            }
        })
    };

    // Bulk archive / delete
    let bulk_archive = {
        Callback::from(move |_| {
            let ids: Vec<i32> = SELECTED.with(|s| s.borrow().iter().copied().collect());
            if ids.is_empty() {
                web_sys::window().unwrap().alert_with_message("No rows selected").ok();
                return;
            }
            spawn_local(async move {
                match ApiClient::archive_entries(&ids).await {
                    Ok(_) => web_sys::window().unwrap().alert_with_message("Archived").ok(),
                    Err(e) => web_sys::window().unwrap().alert_with_message(&format!("archive error: {e}")).ok(),
                }.expect("X098: Shouldn't be none")
            });
        })
    };
    let bulk_delete = {
        Callback::from(move |_| {
            let ids: Vec<i32> = SELECTED.with(|s| s.borrow().iter().copied().collect());
            if ids.is_empty() {
                web_sys::window().unwrap().alert_with_message("No rows selected").ok();
                return;
            }
            if !web_sys::window().unwrap().confirm_with_message("Delete selected entries? This returns amounts to sources.").unwrap_or(false) {
                return;
            }
            spawn_local(async move {
                match ApiClient::delete_entries(&ids).await {
                    Ok(_) => web_sys::window().unwrap().alert_with_message("Deleted").ok(),
                    Err(e) => web_sys::window().unwrap().alert_with_message(&format!("delete error: {e}")).ok(),
                }.expect("X097: Shouldn't be none")
            });
        })
    };

    // Inline create (tfoot)
    let create_click = {
        let c_desc_ref = c_desc_ref.clone();
        let c_long_ref = c_long_ref.clone();
        let c_target_ref = c_target_ref.clone();
        let c_category_ref = c_category_ref.clone();
        let c_amount_ref = c_amount_ref.clone();
        let c_currency_ref = c_currency_ref.clone();
        let c_type_ref = c_type_ref.clone();
        let c_source_ref = c_source_ref.clone();
        let c_source_amount_ref = c_source_amount_ref.clone();
        let c_secondary_ref = c_secondary_ref.clone();
        let c_secondary_amount_ref = c_secondary_amount_ref.clone();
        let c_date_ref = c_date_ref.clone();

        Callback::from(move |_| {
            macro_rules! sval { ($r:expr) => {
                $r.cast::<HtmlInputElement>().map(|i| i.value())
                 .or_else(|| $r.cast::<HtmlSelectElement>().map(|s| s.value()))
                 .unwrap_or_default().trim().to_string()
            }}
            macro_rules! dec_opt { ($s:expr) => { if $s.is_empty() { None } else { Decimal::from_str(&$s).ok() } } }

            let req = CreateEntryRequest {
                description: sval!(c_desc_ref),
                long_description: {
                    let v = sval!(c_long_ref);
                    if v.is_empty() { None } else { Some(v) }
                },
                target: {
                    let v = sval!(c_target_ref);
                    if v.is_empty() { None } else { Some(v) }
                },
                category: sval!(c_category_ref),
                amount: Decimal::from_str(&sval!(c_amount_ref)).unwrap_or(Decimal::ZERO),
                currency: {
                    let v = sval!(c_currency_ref);
                    if v.is_empty() { None } else { Some(v) }
                },
                entry_type: match sval!(c_type_ref).as_str() {
                    "Spend" => EntryType::Spend,
                    "Income" => EntryType::Income,
                    "Lend" => EntryType::Lend,
                    "Borrow" => EntryType::Borrow,
                    "Convert" => EntryType::Convert,
                    _ => EntryType::Spend,
                },
                source: sval!(c_source_ref),
                source_amount: dec_opt!(sval!(c_source_amount_ref)),
                secondary_source: {
                    let v = sval!(c_secondary_ref);
                    if v.is_empty() { None } else { Some(v) }
                },
                secondary_source_amount: dec_opt!(sval!(c_secondary_amount_ref)),
                date: sval!(c_date_ref),
            };

            spawn_local(async move {
                match ApiClient::create_entry(&req).await {
                    Ok(_) => web_sys::window().unwrap().alert_with_message("Created").ok(),
                    Err(e) => web_sys::window().unwrap().alert_with_message(&format!("create error: {e}")).ok(),
                }.expect("X096: Shouldn't be none")
            });
        })
    };

    // --- View ---
    html! {
      <>
        <div ref={stats_ref.clone()}></div>

        <div class="card" style="padding:0;">
          <table class="table" ref={table_ref}>
            <thead>
              // Search row (server-side)
              <tr>
                <th style="width:28px;"><input type="checkbox" onclick={select_all_click} /></th>
                <th colspan="3">
                  <input class="input" type="text" placeholder="Search description…" ref={q_desc_ref} />
                </th>
                <th>
                  <select class="select" ref={q_currency_ref}>
                    <option value="">{ "Any currency" }</option>
                  </select>
                </th>
                <th>
                  <input class="input" type="text" placeholder="amount=…" ref={q_amount_eq_ref} />
                </th>
                <th>
                  <div class="toolbar">
                    <input class="input" type="text" placeholder="min…" ref={q_amount_min_ref} />
                    <input class="input" type="text" placeholder="max…" ref={q_amount_max_ref} />
                  </div>
                </th>
                <th>
                  <select class="select" ref={q_type_ref}>
                    <option value="">{ "Any type" }</option>
                    <option>{ "Spend" }</option><option>{ "Income" }</option>
                    <option>{ "Lend" }</option><option>{ "Borrow" }</option><option>{ "Convert" }</option>
                  </select>
                </th>
                <th><input class="input" type="text" placeholder="source" ref={q_source_ref} /></th>
                <th><input class="input" type="text" placeholder="secondary" ref={q_secondary_ref} /></th>
                <th><input class="input" type="text" placeholder="category" ref={q_category_ref} /></th>
                <th>
                  <div class="toolbar">
                    <input class="input" type="date" ref={q_date_eq_ref} />
                    <select class="select" ref={q_preset_ref}>
                      <option value="">{ "Preset…" }</option>
                      <option value="last3">{ "Last 3 months" }</option>
                    </select>
                  </div>
                </th>
                <th>
                  <div class="toolbar">
                    <input class="input" type="date" ref={q_date_after_ref} />
                    <input class="input" type="date" ref={q_date_before_ref} />
                  </div>
                </th>
                <th>
                  <div class="toolbar">
                    <label style="display:flex;align-items:center;gap:.4rem;">
                      <input type="checkbox" ref={server_toggle_ref} />
                      { "Server sort & search" }
                    </label>
                    <button class="button primary" onclick={search_click}>{ "Search" }</button>
                    <button class="button" onclick={recompute_selection_stats}>{ "Selection stats" }</button>
                    <button class="button warn" onclick={{
                        let sh = sort_handler.clone();
                        Callback::from(move |_| sh.emit("date_asc"))
                    }}>{ "Sort date↑" }</button>
                    <button class="button warn" onclick={{
                        let sh = sort_handler.clone();
                        Callback::from(move |_| sh.emit("date_desc"))
                    }}>{ "Sort date↓" }</button>
                  </div>
                </th>
              </tr>

              // Client-side filters row (applies to current in-memory result)
              <tr>
                <th></th>
                <th><input class="input" type="date" ref={f_date_eq_ref.clone()} oninput={filter_apply.clone()} /></th>
                <th><input class="input" type="text" placeholder="desc contains…" ref={f_desc_ref.clone()} oninput={filter_apply.clone()} /></th>
                <th><input class="input" type="text" placeholder="category" ref={f_category_ref.clone()} oninput={filter_apply.clone()} /></th>
                <th>
                  <select class="select" ref={f_currency_ref.clone()} onchange={filter_apply_event.clone()}>
                    <option value=""></option>
                  </select>
                </th>
                <th>
                  <div class="toolbar">
                    <input class="input" type="text" placeholder="= amount" ref={f_amount_eq_ref.clone()} oninput={filter_apply.clone()} />
                  </div>
                </th>
                <th>
                  <div class="toolbar">
                    <input class="input" type="text" placeholder="min" ref={f_amount_min_ref.clone()} oninput={filter_apply.clone()} />
                    <input class="input" type="text" placeholder="max" ref={f_amount_max_ref.clone()} oninput={filter_apply.clone()} />
                  </div>
                </th>
                <th>
                  <select class="select" ref={f_type_ref.clone()} onchange={filter_apply_event.clone()}>
                    <option value=""></option>
                    <option>{ "Spend" }</option><option>{ "Income" }</option>
                    <option>{ "Lend" }</option><option>{ "Borrow" }</option><option>{ "Convert" }</option>
                  </select>
                </th>
                <th><input class="input" type="text" placeholder="source" ref={f_source_ref.clone()} oninput={filter_apply.clone()} /></th>
                <th><input class="input" type="text" placeholder="secondary" ref={f_secondary_ref.clone()} oninput={filter_apply.clone()} /></th>
                <th></th>
                <th colspan="3">
                  <div class="toolbar">
                    <input class="input" type="date" ref={f_date_after_ref.clone()} oninput={filter_apply.clone()} />
                    <input class="input" type="date" ref={f_date_before_ref.clone()} oninput={filter_apply.clone()} />
                    <button class="button" onclick={{
                        let sh = sort_handler.clone();
                        Callback::from(move |_| sh.emit("amount_asc"))
                    }}>{ "Sort amount↑" }</button>
                    <button class="button" onclick={{
                        let sh = sort_handler.clone();
                        Callback::from(move |_| sh.emit("amount_desc"))
                    }}>{ "Sort amount↓" }</button>
                    <button class="button ok" onclick={bulk_archive.clone()}>{ "Archive selected" }</button>
                    <button class="button danger" onclick={bulk_delete.clone()}>{ "Delete selected" }</button>
                  </div>
                </th>
              </tr>

              // Column headers row
              <tr>
                <th></th>
                <th>{ "Date" }</th>
                <th>{ "Description" }</th>
                <th>{ "Category" }</th>
                <th>{ "Currency" }</th>
                <th style="text-align:right;">{ "Amount" }</th>
                <th style="text-align:right;">{ "Amount (fixed)" }</th>
                <th>{ "Type" }</th>
                <th>{ "Source" }</th>
                <th>{ "Secondary" }</th>
                <th style="text-align:right;">{ "Sec. Amt" }</th>
                <th>{ "Created" }</th>
                <th>{ "Archived" }</th>
                <th>{ "Actions" }</th>
              </tr>
            </thead>

            <tbody ref={tbody_ref}></tbody>

            <tfoot>
              <tr>
                <td></td>
                <td><input class="input" type="date" ref={c_date_ref} /></td>
                <td>
                  <div class="toolbar">
                    <input class="input" type="text" placeholder="description" ref={c_desc_ref} />
                    <input class="input" type="text" placeholder="long desc" ref={c_long_ref} />
                  </div>
                </td>
                <td><input class="input" type="text" placeholder="category" ref={c_category_ref} /></td>
                <td><input class="input" type="text" placeholder="currency (opt)" ref={c_currency_ref} /></td>
                <td><input class="input" type="text" placeholder="amount" ref={c_amount_ref} /></td>
                <td></td>
                <td>
                  <select class="select" ref={c_type_ref}>
                    <option>{ "Spend" }</option><option>{ "Income" }</option>
                    <option>{ "Lend" }</option><option>{ "Borrow" }</option><option>{ "Convert" }</option>
                  </select>
                </td>
                <td><input class="input" type="text" placeholder="source" ref={c_source_ref} /></td>
                <td><input class="input" type="text" placeholder="secondary (opt)" ref={c_secondary_ref} /></td>
                <td><input class="input" type="text" placeholder="secondary amt (opt)" ref={c_secondary_amount_ref} /></td>
                <td><input class="input" type="text" placeholder="target (opt)" ref={c_target_ref} /></td>
                <td></td>
                <td>
                  <button ref={create_button_ref} class="button ok" onclick={create_click}>{ "Create" }</button>
                </td>
              </tr>
            </tfoot>
          </table>
        </div>
      </>
    }
}

*/