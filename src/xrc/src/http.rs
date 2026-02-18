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
            #[allow(deprecated)]
            args: CanisterHttpRequestArgument {
                url: Default::default(),
                max_response_bytes: Default::default(),
                headers: vec![HttpHeader {
                    name: "User-Agent".to_string(),
                    value: "Exchange Rate Canister".to_string(),
                }],
                body: Default::default(),
                #[allow(deprecated)]
                method: HttpMethod::GET,
                transform: None,
            },
        }
    }

    /// A simple wrapper to assign the URL with the `GET` method.
    #[allow(deprecated)]
    pub fn get(self, url: &str) -> Self {
        self.url(url).method(HttpMethod::GET)
    }

    /// Updates the HTTP method in the `args` field.
    #[allow(deprecated)]
    pub fn method(mut self, http_method: HttpMethod) -> Self {
        self.args.method = http_method;
        self
    }

    /// Adds HTTP headers for the request
    pub fn add_headers(mut self, headers: Vec<(String, String)>) -> Self {
        #[allow(deprecated)]
        self.args
            .headers
            .extend(headers.iter().map(|(name, value)| HttpHeader {
                name: name.to_string(),
                value: value.to_string(),
            }));
        self
    }

    /// Updates the URL in the `args` field.
    #[allow(deprecated)]
    pub fn url(mut self, url: &str) -> Self {
        self.args.url = String::from(url);
        self
    }

    /// Updates the transform context of the request.
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
    #[allow(deprecated)]
    pub async fn send(self) -> Result<HttpResponse, String> {
        http_request(self.args, self.cycles)
            .await
            .map(|(response,)| response)
            .map_err(|(_rejection_code, message)| message)
    }
}
