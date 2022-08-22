mod types;

use ic_cdk::export::candid::candid_method;

use jaq_core::{parse, Ctx, Definitions, RcIter, Val};
use jaq_std::std;
use serde_json::{from_str, Value};

#[ic_cdk_macros::query]
#[candid_method(query)]
fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}

#[ic_cdk_macros::query]
#[candid_method(query)]
fn extract_rate(response: String, filter: String) -> u64 {
    let input: Value = from_str(response.as_str()).unwrap();

    // Add required filters to the Definitions core.
    let mut definitions = Definitions::core();

    let used_defs = std()
        .into_iter()
        .filter(|d| d.name == "map" || d.name == "select");

    for def in used_defs {
        definitions.insert(def, &mut vec![]);
    }

    // Parse the filter in the context of the given definitions.
    let mut errs = Vec::new();
    let f = parse::parse(&filter, parse::main()).0.unwrap();
    let f = definitions.finish(f, Vec::new(), &mut errs);
    assert_eq!(errs, Vec::new());

    let inputs = RcIter::new(core::iter::empty());

    // Extract the output.
    let mut out = f.run(Ctx::new([], &inputs), Val::from(input));
    let output = out.next().unwrap().unwrap();

    match output {
        Val::Num(rc_number) => ((*rc_number).as_f64().unwrap() * 100.0) as u64,
        _ => 0, // Return zero for now.
    }
}

#[ic_cdk_macros::update]
#[candid_method(update)]
fn get_exchange_rate(_request: types::GetExchangeRateRequest) -> types::GetExchangeRateResult {
    todo!()
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::*;

    use ic_cdk::export::candid;

    #[test]
    fn check_candid_compatibility() {
        candid::export_service!();
        // Pull in the rust-generated interface and candid file interface.
        let new_interface = __export_service();
        let old_interface =
            PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("xrc.did");

        candid::utils::service_compatible(
            candid::utils::CandidSource::Text(&new_interface),
            candid::utils::CandidSource::File(old_interface.as_path()),
        )
        .expect("Service incompatibility found");
    }
}
