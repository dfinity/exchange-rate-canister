use jaq_core::{parse, Ctx, Definitions, RcIter, Val};
use jaq_std::std;

use crate::ExtractError;

/// This function extracts a jaq::Val from the provided JSON value given a `jq`-like filter.
#[allow(dead_code)]
pub fn extract(bytes: &[u8], filter: &str) -> Result<Val, ExtractError> {
    let input: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| ExtractError::JsonDeserialize(e.to_string()))?;

    // Add required filters to the Definitions core.
    let mut definitions = Definitions::core();

    for def in std() {
        definitions.insert(def, &mut vec![]);
    }

    // Parse the filter in the context of the given definitions.
    let (maybe_parsed_filter, errors) = parse::parse(filter, parse::main());
    if !errors.is_empty() {
        return Err(ExtractError::MalformedFilterExpression {
            filter: filter.to_string(),
            errors: errors.iter().map(|s| s.to_string()).collect(),
        });
    }
    let parsed_filter_definition =
        maybe_parsed_filter.expect("Errors is empty. There should be a parsed filter.");

    let mut errors = Vec::new();
    let parsed_filter = definitions.finish(parsed_filter_definition, Vec::new(), &mut errors);

    if !errors.is_empty() {
        return Err(ExtractError::MalformedFilterExpression {
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

    /// Tests a good filter that can properly select from a given JSON value.
    #[test]
    fn good_filter() {
        let result = extract(VALID_JSON.as_bytes(), ".[0][3]");
        assert!(matches!(result, Ok(Val::Num(n)) if n.to_string() == "6.527"));
    }

    /// Tests that an invalid filter expression will cause an error.
    #[test]
    fn malformed_filter_expression() {
        let bad_filter = ".[0}";
        let result = extract(VALID_JSON.as_bytes(), bad_filter);
        assert!(
            matches!(result, Err(ExtractError::MalformedFilterExpression { filter, errors: _ }) if filter == bad_filter),
        );
    }

    /// Tests a good filter with a bad selector will cause an error to occur.
    /// In this specific case, attempting to access a property for a null value.
    #[test]
    fn good_filter_with_bad_selector() {
        let bad_filter = ".[2][3]";
        let result = extract(VALID_JSON.as_bytes(), bad_filter);
        assert!(
            matches!(result, Err(ExtractError::Extraction { filter, error }) if filter == bad_filter && error == "cannot index null"),
        );
    }
}
