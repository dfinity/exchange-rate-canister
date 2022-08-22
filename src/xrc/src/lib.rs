use jaq_core::{parse, Ctx, Definitions, RcIter, Val};
use jaq_std::std;
use serde_json::{from_str, Value};

#[ic_cdk_macros::query]
fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}

#[ic_cdk_macros::query]
fn extract_rate(response: String, filter: String) -> u64
{
    let input : Value = from_str(response.as_str()).unwrap();

    // Add required filters to the Definitions core.
    let mut definitions = Definitions::core();

    let used_defs = std().into_iter().filter(|d| d.name == "map" || d.name == "select");

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
        Val::Num(rc_number) => {
            ((*rc_number).as_f64().unwrap() * 100.0) as u64
        },
        _ => 0  // Return zero for now.
    }
}
