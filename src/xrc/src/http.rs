use async_trait::async_trait;
use ic_cdk::{
    api::management_canister::http_request::{
        http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse,
        TransformContext, TransformFunc,
    },
    export::candid::Func,
    id,
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
                transform: None,
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

    /// Updates the transform context of the request.
    pub fn transform_context(mut self, method: &str, context: Vec<u8>) -> Self {
        let context = TransformContext {
            function: TransformFunc(Func {
                principal: id(),
                method: method.to_string(),
            }),
            context,
        };

        self.args.transform = Some(context);
        self
    }

    /// Updates the max_response_bytes of the request.
    pub fn max_response_bytes(mut self, max_response_bytes: u64) -> Self {
        self.args.max_response_bytes = Some(max_response_bytes);
        self
    }

    /// Wraps around `http_request` to issue a request to the `http_request` endpoint.
    pub async fn send(self) -> Result<HttpResponse, String> {
        http_request(self.args)
            .await
            .map(|(response,)| response)
            .map_err(|(_rejection_code, message)| message)
    }

    pub fn build(self) -> CanisterHttpRequestArgument {
        self.args
    }
}

#[async_trait]
pub(crate) trait HttpRequestClient {
    async fn call(&self, arg: CanisterHttpRequestArgument) -> Result<HttpResponse, String>;
}

pub(crate) struct HttpRequestClientImpl;

#[async_trait]
impl HttpRequestClient for HttpRequestClientImpl {
    async fn call(&self, arg: CanisterHttpRequestArgument) -> Result<HttpResponse, String> {
        http_request(arg)
            .await
            .map(|(response,)| response)
            .map_err(|(_rejection_code, message)| message)
    }
}

#[cfg(test)]
pub(crate) mod test {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use super::*;

    pub(crate) struct MockHttpRequestClientImpl {
        requests: Arc<Mutex<Vec<CanisterHttpRequestArgument>>>,
        responses: HashMap<String, HttpResponse>,
    }

    #[async_trait]
    impl HttpRequestClient for MockHttpRequestClientImpl {
        async fn call(&self, arg: CanisterHttpRequestArgument) -> Result<HttpResponse, String> {
            let response = self
                .responses
                .get(&arg.url)
                .cloned()
                .ok_or_else(|| "Unable to find response for provided URL".to_string())?;
            self.requests
                .lock()
                .expect("failed to lock requests")
                .push(arg);
            Ok(response)
        }
    }
}
