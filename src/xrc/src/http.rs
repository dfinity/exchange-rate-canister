use candid::Func;
use ic_cdk::{
    api::canister_self,
};
#[allow(deprecated)]
use ic_cdk::{
    api::management_canister::http_request::{
        http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse,
        TransformContext, TransformFunc,
    },
};

/// The `User-Agent` sent on every outcall unless a source overrides it via
/// [`CanisterHttpRequest::user_agent`].
pub(crate) const DEFAULT_USER_AGENT: &str = "Exchange Rate Canister";

/// Used to build a request to the Management Canister's `http_request` method.
// TODO(DEFI-2648): Migrate to non-deprecated.
#[allow(deprecated)]
pub struct CanisterHttpRequest {
    args: CanisterHttpRequestArgument,
    cycles: u128,
}

impl Default for CanisterHttpRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl CanisterHttpRequest {
    /// Creates a new request to be built up by having
    pub fn new() -> Self {
        Self {
            cycles: 0,
            // TODO(DEFI-2648): Migrate to non-deprecated.
            #[allow(deprecated)]
            args: CanisterHttpRequestArgument {
                url: Default::default(),
                max_response_bytes: Default::default(),
                headers: vec![HttpHeader {
                    name: "User-Agent".to_string(),
                    value: DEFAULT_USER_AGENT.to_string(),
                }],
                body: Default::default(),
                // TODO(DEFI-2648): Migrate to non-deprecated.
                #[allow(deprecated)]
                method: HttpMethod::GET,
                transform: None,
            },
        }
    }

    /// A simple wrapper to assign the URL with the `GET` method.
    // TODO(DEFI-2648): Migrate to non-deprecated.
    #[allow(deprecated)]
    pub fn get(self, url: &str) -> Self {
        self.url(url).method(HttpMethod::GET)
    }

    /// Updates the HTTP method in the `args` field.
    // TODO(DEFI-2648): Migrate to non-deprecated.
    #[allow(deprecated)]
    pub fn method(mut self, http_method: HttpMethod) -> Self {
        self.args.method = http_method;
        self
    }

    /// Adds HTTP headers for the request.
    pub fn add_headers(mut self, headers: Vec<(String, String)>) -> Self {
        // TODO(DEFI-2648): Migrate to non-deprecated.
        #[allow(deprecated)]
        self.args.headers.extend(
            headers
                .into_iter()
                .map(|(name, value)| HttpHeader { name, value }),
        );
        self
    }

    /// Sets the `User-Agent` header for the request, overriding the default set
    /// in [`CanisterHttpRequest::new`]. `User-Agent` is a singleton header, so
    /// this replaces the single value rather than appending another field.
    pub fn user_agent(mut self, user_agent: &str) -> Self {
        // TODO(DEFI-2648): Migrate to non-deprecated.
        #[allow(deprecated)]
        match self
            .args
            .headers
            .iter_mut()
            .find(|header| header.name.eq_ignore_ascii_case("User-Agent"))
        {
            Some(header) => header.value = user_agent.to_string(),
            None => self.args.headers.push(HttpHeader {
                name: "User-Agent".to_string(),
                value: user_agent.to_string(),
            }),
        }
        self
    }

    /// Updates the URL in the `args` field.
    // TODO(DEFI-2648): Migrate to non-deprecated.
    #[allow(deprecated)]
    pub fn url(mut self, url: &str) -> Self {
        self.args.url = String::from(url);
        self
    }

    /// Updates the transform context of the request.
    // TODO(DEFI-2648): Migrate to non-deprecated.
    #[allow(deprecated)]
    pub fn transform_context(mut self, method: &str, context: Vec<u8>) -> Self {
        let context = TransformContext {
            function: TransformFunc(Func {
                principal: canister_self(),
                method: method.to_string(),
            }),
            context,
        };

        self.args.transform = Some(context);
        self
    }

    /// Updates the max_response_bytes of the request.
    // TODO(DEFI-2648): Migrate to non-deprecated.
    #[allow(deprecated)]
    pub fn max_response_bytes(mut self, max_response_bytes: u64) -> Self {
        self.args.max_response_bytes = Some(max_response_bytes);
        self
    }

    pub fn cycles(mut self, cycles: u128) -> Self {
        self.cycles = cycles;
        self
    }

    /// Wraps around `http_request` to issue a request to the `http_request` endpoint.
    // TODO(DEFI-2648): Migrate to non-deprecated.
    #[allow(deprecated)]
    pub async fn send(self) -> Result<HttpResponse, String> {
        http_request(self.args, self.cycles)
            .await
            .map(|(response,)| response)
            .map_err(|(_rejection_code, message)| message)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Collects the values of every header whose name matches `name`
    /// (case-insensitive). Lets tests assert by header name instead of relying
    /// on the position or total count of headers, and confines the deprecated
    /// field access to one place.
    fn header_values(request: &CanisterHttpRequest, name: &str) -> Vec<String> {
        // TODO(DEFI-2648): Migrate to non-deprecated.
        #[allow(deprecated)]
        request
            .args
            .headers
            .iter()
            .filter(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.clone())
            .collect()
    }

    /// A new request carries the default `User-Agent` header.
    #[test]
    fn default_user_agent() {
        let request = CanisterHttpRequest::new();
        assert_eq!(
            header_values(&request, "User-Agent"),
            vec!["Exchange Rate Canister".to_string()]
        );
    }

    /// `add_headers` appends a header whose name is not already present, leaving
    /// the default `User-Agent` in place.
    #[test]
    fn add_headers_appends_new_header() {
        let request = CanisterHttpRequest::new()
            .add_headers(vec![("Accept".to_string(), "application/json".to_string())]);
        assert_eq!(
            header_values(&request, "Accept"),
            vec!["application/json".to_string()]
        );
        assert_eq!(
            header_values(&request, "User-Agent"),
            vec!["Exchange Rate Canister".to_string()]
        );
    }

    /// `user_agent` overrides the default `User-Agent` in place, leaving exactly
    /// one `User-Agent` header that carries the new value.
    #[test]
    fn user_agent_overrides_default() {
        let request = CanisterHttpRequest::new().user_agent("curl/8.0");
        assert_eq!(
            header_values(&request, "User-Agent"),
            vec!["curl/8.0".to_string()]
        );
    }
}
