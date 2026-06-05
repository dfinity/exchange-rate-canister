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
                    value: "Exchange Rate Canister".to_string(),
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

    /// Adds HTTP headers for the request, replacing any existing header with
    /// the same (case-insensitive) name. This lets a source override a default
    /// header such as the `User-Agent` set in [`CanisterHttpRequest::new`].
    pub fn add_headers(mut self, headers: Vec<(String, String)>) -> Self {
        for (name, value) in headers {
            // TODO(DEFI-2648): Migrate to non-deprecated.
            #[allow(deprecated)]
            match self
                .args
                .headers
                .iter_mut()
                .find(|header| header.name.eq_ignore_ascii_case(&name))
            {
                Some(header) => header.value = value,
                None => self.args.headers.push(HttpHeader { name, value }),
            }
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
    // The tests below read the deprecated `args.headers` / `HttpHeader` fields.
    // TODO(DEFI-2648): Migrate to non-deprecated.
    #![allow(deprecated)]

    use super::*;

    /// A new request carries the default `User-Agent` header.
    #[test]
    fn default_user_agent() {
        let request = CanisterHttpRequest::new();
        let headers = &request.args.headers;
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name, "User-Agent");
        assert_eq!(headers[0].value, "Exchange Rate Canister");
    }

    /// `add_headers` appends headers whose name is not already present.
    #[test]
    fn add_headers_appends_new_header() {
        let request = CanisterHttpRequest::new()
            .add_headers(vec![("Accept".to_string(), "application/json".to_string())]);
        let headers = &request.args.headers;
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[1].name, "Accept");
        assert_eq!(headers[1].value, "application/json");
    }

    /// `add_headers` replaces a header with a matching (case-insensitive) name
    /// rather than appending a duplicate, so a source can override the default
    /// `User-Agent`.
    #[test]
    fn add_headers_replaces_existing_header() {
        let request = CanisterHttpRequest::new()
            .add_headers(vec![("user-agent".to_string(), "curl/8.0".to_string())]);
        let headers = &request.args.headers;
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name, "User-Agent");
        assert_eq!(headers[0].value, "curl/8.0");
    }
}
