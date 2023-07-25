use candid::{CandidType, Deserialize};
use serde_bytes::ByteBuf;

type HeaderField = (String, String);

/// An incoming request to the canister through its `http_request` endpoint.
#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct HttpRequest {
    /// The http method (GET, POST, etc.)
    pub method: String,
    /// The URL endpoint attempting to be accessed.
    pub url: String,
    /// The HTTP headers of the request.
    pub headers: Vec<(String, String)>,
    /// The body of the request as bytes.
    pub body: ByteBuf,
}

/// An outgoing response from the canister through its `http_request` endpoint.
#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct HttpResponse {
    /// The returning status code.
    pub status_code: u16,
    /// The HTTP response headers.
    pub headers: Vec<HeaderField>,
    /// The HTTP response body.
    pub body: ByteBuf,
}
