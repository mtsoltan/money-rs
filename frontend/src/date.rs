use chrono::{Datelike, Days, Months, NaiveDate};
use wasm_bindgen_futures::js_sys;

/// After and before are both inclusive, just like they are in EntryQuery
#[derive(Debug, Clone)]
pub struct DateRange {
    /// inclusive lower bound (YYYY-MM-DD)
    pub after: String,
    /// inclusive upper bound (YYYY-MM-DD)
    pub before: String,
}

pub enum Preset {
    ThisMonthToNow,
    LastMonth,
    Last3MonthsToNow,
    ThisYearToNow,
    Last12MonthsToNow,
}

/// After and before are both inclusive, just like they are in EntryQuery
pub fn preset_range(p: Preset, now: NaiveDate) -> DateRange {
    match p {
        Preset::ThisMonthToNow => {
            let start = now.with_day(1).expect("X015: valid month");
            DateRange { after: start.format("%F").to_string(), before: now.format("%F").to_string() }
        }
        Preset::LastMonth => {
            let start = now.checked_sub_months(Months::new(1)).expect("X015: valid month").with_day(1).expect("X015: valid month");
            let end = now.with_day(1).expect("valid month").checked_sub_days(Days::new(1)).expect("X014: valid days");
            DateRange { after: start.format("%F").to_string(), before: end.format("%F").to_string() }
        }
        Preset::Last3MonthsToNow => {
            let start = now.checked_sub_months(Months::new(3)).expect("X015: valid month").with_day(1).expect("X015: valid month");
            DateRange { after: start.format("%F").to_string(), before: now.format("%F").to_string() }
        }
        Preset::ThisYearToNow => {
            let start = now.with_month(1).expect("X015: valid month").with_day(1).expect("X015: valid month");
            DateRange { after: start.format("%F").to_string(), before: now.format("%F").to_string() }
        }
        Preset::Last12MonthsToNow => {
            let start = now.checked_sub_months(Months::new(12)).expect("X015: valid month").with_day(1).expect("X015: valid month");
            DateRange { after: start.format("%F").to_string(), before: now.format("%F").to_string() }
        }
    }
}

pub fn js_now_to_naive_date() -> NaiveDate {
    let now = js_sys::Date::new_0();
    let y = now.get_utc_full_year() as i32;
    let m = now.get_utc_month() + 1;
    let d = now.get_utc_date();
    NaiveDate::from_ymd_opt(y, m, d).expect("X016: valid date")
}
