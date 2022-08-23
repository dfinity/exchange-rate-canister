use ic_cdk::api::management_canister::http_request::{
    http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse,
};
pub struct CanisterHttpRequest {
    args: CanisterHttpRequestArgument,
}

impl CanisterHttpRequest {
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
                http_method: HttpMethod::GET,
                transform_method_name: Default::default(),
            },
        }
    }

    pub fn get(self, url: &str) -> Self {
        self.url(url).method(HttpMethod::GET)
    }

    pub fn method(mut self, http_method: HttpMethod) -> Self {
        self.args.http_method = http_method;
        self
    }

    pub fn url(mut self, url: &str) -> Self {
        self.args.url = String::from(url);
        self
    }

    pub async fn send(self) -> HttpResponse {
        http_request(self.args).await.unwrap().0
    }
}
