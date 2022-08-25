use jaq_core::{parse, Ctx, Definitions, RcIter, Val};
use jaq_std::std;

#[derive(Debug)]
pub enum ExtractError {
    ParseFilter { filter: String, errors: Vec<String> },
    Extraction { filter: String, error: String },
}

impl core::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::ParseFilter { filter, errors } => {
                let joined_errors = errors.join("\n");
                write!(f, "Parsing filter ({filter}) failed: {joined_errors}")
            }
            ExtractError::Extraction { filter, error } => {
                write!(
                    f,
                    "Extracting values with filter ({filter}) failed: {error}"
                )
            }
        }
    }
}

pub fn extract(input: serde_json::Value, filter: &str) -> Result<Val, ExtractError> {
    // Add required filters to the Definitions core.
    let mut definitions = Definitions::core();

    let used_defs = std()
        .into_iter()
        .filter(|d| d.name == "map" || d.name == "select");

    for def in used_defs {
        definitions.insert(def, &mut vec![]);
    }

    // Parse the filter in the context of the given definitions.
    let (maybe_parsed_filter, errors) = parse::parse(filter, parse::main());
    if !errors.is_empty() {
        return Err(ExtractError::ParseFilter {
            filter: filter.to_string(),
            errors: errors.iter().map(|s| s.to_string()).collect(),
        });
    }
    let parsed_filter_definition =
        maybe_parsed_filter.expect("Errors is empty. There should be a parsed filter.");

    let mut errors = Vec::new();
    let parsed_filter = definitions.finish(parsed_filter_definition, Vec::new(), &mut errors);

    if !errors.is_empty() {
        return Err(ExtractError::ParseFilter {
            filter: filter.to_string(),
            errors: errors.iter().map(|s| s.to_string()).collect(),
        });
    }

    let inputs = RcIter::new(core::iter::empty());

    // Extract the output.
    let mut out = parsed_filter.run(Ctx::new([], &inputs), Val::from(input));
    match out.next() {
        Some(result) => match result {
            Ok(val) => Ok(val),
            Err(error) => Err(ExtractError::Extraction {
                filter: filter.to_string(),
                error: error.to_string(),
            }),
        },
        None => Ok(Val::Null),
    }
}

#[cfg(test)]
mod test {

    use super::*;

    const VALID_JSON: &str = "[[1661426460,6.527,6.539,6.527,6.539,235.6124],[1661426400,6.528,6.542,6.542,6.528,246.9019]]";

    #[test]
    fn good_filter() {
        let input: serde_json::Value =
            serde_json::from_str(VALID_JSON).expect("valid JSON was expected");
        let result = extract(input, ".[0][3]");
        assert!(matches!(result, Ok(Val::Num(n)) if n.to_string() == "6.527"));
    }

    #[test]
    fn malformed_filter_expression() {
        let bad_filter = ".[0}";
        let input: serde_json::Value =
            serde_json::from_str(VALID_JSON).expect("valid JSON was expected");
        let result = extract(input, bad_filter);
        assert!(
            matches!(result, Err(ExtractError::ParseFilter { filter, errors: _ }) if filter == bad_filter),
        );
    }

    #[test]
    fn bad_selector_for_filter() {
        let bad_filter = ".[2][3]";
        let input: serde_json::Value =
            serde_json::from_str(VALID_JSON).expect("valid JSON was expected");
        let result = extract(input, bad_filter);
        assert!(
            matches!(result, Err(ExtractError::Extraction { filter, error }) if filter == bad_filter && error == "cannot index null"),
        );
    }
}
