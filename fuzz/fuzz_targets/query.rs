//! The picker query (column filters such as `name:foo`): parsing never panics, whatever the
//! columns.
#![no_main]
use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use waymaker::nucleo::query::PickerQuery;

fuzz_target!(|input: (Vec<String>, u8, &str)| {
    let (mut columns, primary, query) = input;
    columns.truncate(8);
    if columns.is_empty() {
        columns.push(String::new());
    }
    let primary = primary as usize % columns.len();
    let mut picker_query = PickerQuery::new(columns.into_iter().map(Arc::from), primary);
    picker_query.parse(query);
});
