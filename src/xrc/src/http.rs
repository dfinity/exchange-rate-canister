use ic_cdk::export::{
    candid::{self, CandidType},
    serde::{Deserialize, Serialize},
    Principal,
};

const IC_ENDPOINT: &str = "http_request";

#[derive(CandidType, Clone, Deserialize, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Debug, PartialEq, CandidType, Eq, Hash, Serialize, Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    HEAD,
}

#[derive(CandidType, Deserialize, Debug)]
struct CanisterHttpRequestArgs {
    url: String,
    max_response_bytes: Option<u64>,
    headers: Vec<HttpHeader>,
    body: Option<Vec<u8>>,
    http_method: HttpMethod,
    transform_method_name: Option<String>,
}

impl Default for CanisterHttpRequestArgs {
    fn default() -> Self {
        Self {
            url: Default::default(),
            max_response_bytes: Default::default(),
            headers: vec![HttpHeader {
                name: "User-Agent".to_string(),
                value: "Exchange Rate Canister".to_string(),
            }],
            body: Default::default(),
            http_method: HttpMethod::GET,
            transform_method_name: Default::default(),
        }
    }
}

#[derive(CandidType, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CanisterHttpResponsePayload {
    pub status: u64,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
}

pub struct CanisterHttpRequest {
    args: CanisterHttpRequestArgs,
    cycles: u64,
}

impl CanisterHttpRequest {
    pub fn new() -> Self {
        let args = CanisterHttpRequestArgs::default();
        Self {
            args,
            cycles: Default::default(),
        }
    }

    pub fn get(self, url: &str) -> Self {
        self.url(url).method(HttpMethod::GET)
    }

    #[allow(dead_code)]
    pub fn post(self, url: &str) -> Self {
        self.url(url).method(HttpMethod::POST)
    }

    #[allow(dead_code)]
    pub fn head(self, url: &str) -> Self {
        self.url(url).method(HttpMethod::HEAD)
    }

    pub fn method(mut self, http_method: HttpMethod) -> Self {
        self.args.http_method = http_method;
        self
    }

    #[allow(dead_code)]
    pub fn cycles(mut self, cycles: u64) -> Self {
        self.cycles = cycles;
        self
    }

    pub fn url(mut self, url: &str) -> Self {
        self.args.url = String::from(url);
        self
    }

    #[allow(dead_code)]
    pub fn headers(mut self, headers: Vec<HttpHeader>) -> Self {
        self.args.headers = headers;
        self
    }

    #[allow(dead_code)]
    pub fn body(mut self, body: Option<Vec<u8>>) -> Self {
        self.args.body = body;
        self
    }

    pub async fn send(self) -> CanisterHttpResponsePayload {
        let args = candid::utils::encode_one(&self.args).unwrap();
        let bytes = ic_cdk::api::call::call_raw(
            Principal::management_canister(),
            IC_ENDPOINT,
            &args[..],
            self.cycles,
        )
        .await
        .unwrap();
        candid::utils::decode_one(&bytes).unwrap()
    }
}
