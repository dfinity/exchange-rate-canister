use ic_cdk::{export::candid::Func, id};

use crate::canister_http::{
    http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse,
    TransformContext, TransformFunc,
};

/// Used to build a request to the Management Canister's `http_request` method.
pub struct CanisterHttpRequest {
    args: CanisterHttpRequestArgument,
}

impl Default for CanisterHttpRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl CanisterHttpRequest {
    /// Creates a new request to be built up by having
    pub fn new() -> Self {
        let context = TransformContext {
            function: TransformFunc(Func {
                principal: id(),
                method: "transform_http_response".to_string(),
            }),
            context: vec![1, 2, 3],
        };

        Self {
            args: CanisterHttpRequestArgument {
                url: Default::default(),
                max_response_bytes: Default::default(),
                headers: vec![HttpHeader {
                    name: "User-Agent".to_string(),
                    value: "Exchange Rate Canister".to_string(),
                }],
                body: Default::default(),
                method: HttpMethod::GET,
                transform: Some(context),
            },
        }
    }

    /// A simple wrapper to assign the URL with the `GET` method.
    pub fn get(self, url: &str) -> Self {
        self.url(url).method(HttpMethod::GET)
    }

    /// Updates the HTTP method in the `args` field.
    pub fn method(mut self, http_method: HttpMethod) -> Self {
        self.args.method = http_method;
        self
    }

    /// Updates the URL in the `args` field.
    pub fn url(mut self, url: &str) -> Self {
        self.args.url = String::from(url);
        self
    }

    /// Wraps around `http_request` to issue a request to the `http_request` endpoint.
    pub async fn send(self) -> Result<HttpResponse, String> {
        http_request(self.args)
            .await
            .map(|(response,)| response)
            .map_err(|(_rejection_code, message)| message)
    }
}
