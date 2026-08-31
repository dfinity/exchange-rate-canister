//! Hand-rolled Candid binding for the management canister's `flexible_http_request`
//! endpoint — the committee ("flexible") HTTPS outcall that powers the opt-in
//! `live` rate mode.
//!
//! Unlike the classic replicated outcall, a flexible outcall does not require
//! byte-for-byte agreement across replicas: a committee of nodes each perform the
//! outcall independently and the canister receives a **vector** of (possibly
//! differing) responses that it aggregates itself. This lets XRC query the
//! forming minute via real-time ticker endpoints. This module is only the
//! transport binding; aggregation of the per-node response vector lives
//! elsewhere.
//!
//! There is no `ic-cdk` 0.19 helper for `flexible_http_request` and the interface
//! is not yet in the published `ic.did`, so this module hand-encodes the Candid
//! types and calls via the raw [`ic_cdk::call::Call`] builder. The wire types
//! below **mirror** the upstream definition in
//! `rs/types/management_canister_types/src/http.rs` on `dfinity/ic`
//! (branch `eichhorl/flexible-max-response-bytes`; expected to merge to `master`
//! the week of 2026-07-20). They are the single point of change: if the upstream
//! layout shifts before it lands, reconcile the structs here.

use candid::{CandidType, Deserialize, Principal};

use crate::http::DEFAULT_USER_AGENT;

/// The management-canister method that issues a flexible (committee) HTTPS
/// outcall.
const FLEXIBLE_HTTP_REQUEST_METHOD: &str = "flexible_http_request";

/// Number of nodes on XRC's production system subnet (`uzr34-…`), used as the
/// flexible-outcall committee size (`total_requests`/`max_responses`).
///
/// This is a compile-time constant by design: XRC has no runtime subnet-size
/// query and no out-of-band control, so the committee shape is fixed at build
/// time and only changed by rebuild + NNS proposal.
pub const SUBNET_NODE_COUNT: u32 = 34;

/// Committee quorum: the number of nodes that must return an OK response before
/// an exchange is counted, `⌈2N/3⌉ + 1` (clamped to `≤ N`). At the production
/// `N = 34` this is `24`.
///
/// This is the tunable trust/availability knob (a higher value makes it harder
/// for a minority of nodes to skew the median, at the cost of availability when
/// popular endpoints rate-limit part of the committee). It stays a static
/// compile-time constant so behavior is deterministic across replicas and
/// reviewable in a proposal.
pub const MIN_COMMITTEE_RESPONSES: u32 = min_committee_responses(SUBNET_NODE_COUNT);

/// Computes the default committee quorum `⌈2n/3⌉ + 1`, clamped to `≤ n`.
const fn min_committee_responses(n: u32) -> u32 {
    let quorum = (2 * n).div_ceil(3) + 1;
    if quorum > n {
        n
    } else {
        quorum
    }
}

/// Returns an **explicit** full-subnet committee configuration: one request per
/// node, requiring `MIN_COMMITTEE_RESPONSES` OK responses.
///
/// This is the building block for the *explicit-committee* alternative, not the
/// default. [`FlexibleHttpRequest`] defaults to `replication: None` (see its
/// docs) because the replica **rejects** a `total_requests` greater than the
/// live node count `N` rather than clamping it, which makes a hardcoded `N`
/// fragile to subnet-topology changes. Use this only if a future caller decides
/// to pin the committee to keep the tuned `⌈2N/3⌉+1` quorum (accepting that
/// `SUBNET_NODE_COUNT` must then track the real `N`).
pub fn full_subnet_replication() -> ReplicationCounts {
    ReplicationCounts {
        total_requests: SUBNET_NODE_COUNT,
        min_responses: MIN_COMMITTEE_RESPONSES,
        max_responses: SUBNET_NODE_COUNT,
    }
}

/// The number of cycles attached to a flexible outcall.
///
/// Mirrors [`crate::exchanges::Exchange::cycles`]: on the free-cost-schedule
/// production system subnet, attaching `0` works (as XRC already does for
/// classic outcalls); the application-subnet build attaches a flat amount. On a
/// `Normal`-cost-schedule subnet the flexible path is routed to PAYG downstream,
/// which is unimplemented and rejected — so the feature is initially
/// free-subnet-only.
pub fn default_cycles() -> u128 {
    if cfg!(feature = "application-subnet") {
        500_000_000
    } else {
        0
    }
}

// ============================================================================
// Wire types — faithful mirror of the upstream `dfinity/ic` definitions.
// ============================================================================

/// A single HTTP header (`record { name : text; value : text }`).
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct HttpHeader {
    /// Header name.
    pub name: String,
    /// Header value.
    pub value: String,
}

/// The HTTP method of a flexible outcall.
///
/// XRC only ever issues `GET`, but the full variant set is mirrored so the
/// encoded Candid type matches the upstream definition exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum HttpMethod {
    /// `GET`.
    #[serde(rename = "get")]
    Get,
    /// `POST`.
    #[serde(rename = "post")]
    Post,
    /// `HEAD`.
    #[serde(rename = "head")]
    Head,
    /// `PUT`.
    #[serde(rename = "put")]
    Put,
    /// `DELETE`.
    #[serde(rename = "delete")]
    Delete,
    /// `PATCH`.
    #[serde(rename = "patch")]
    Patch,
}

/// A single node's HTTP response as returned inside the flexible result vector
/// (`record { status : nat; headers : vec http_header; body : blob }`).
///
/// Note that `status` is a `nat` (`u128`) here, matching the upstream flexible
/// payload — not the `candid::Nat` used by the classic `ic-cdk` `HttpResponse`.
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CanisterHttpResponsePayload {
    /// HTTP status code (e.g. `200`).
    pub status: u128,
    /// Response headers.
    pub headers: Vec<HttpHeader>,
    /// Raw response body.
    #[serde(with = "serde_bytes")]
    pub body: Vec<u8>,
}

/// Arguments passed to the per-node transform query
/// (`record { response : http_response; context : blob }`).
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransformArgs {
    /// The raw per-node response to be reduced before it is gossiped.
    pub response: CanisterHttpResponsePayload,
    /// Opaque context passed through to the transform.
    #[serde(with = "serde_bytes")]
    pub context: Vec<u8>,
}

mod transform_func {
    // `candid::define_function!` generates a public type whose doc comment cannot
    // be attached through the macro invocation, so this one-item module scopes the
    // `missing_docs` allowance to the generated type alone; it is documented at the
    // re-export below. Mirrors the upstream `dfinity/ic` workaround.
    #![allow(missing_docs)]
    use super::{CanisterHttpResponsePayload, TransformArgs};
    candid::define_function!(pub TransformFunc : (TransformArgs) -> (CanisterHttpResponsePayload) query);
}

/// Candid `func` reference to the query method that reduces each per-node
/// response before it is gossiped:
/// `func (record { response : http_response; context : blob }) -> (http_response) query`.
pub use transform_func::TransformFunc;

/// The transform applied to each node's response, retained in `live` mode solely
/// to minimize gossiped bytes (`record { function : func …; context : blob }`).
#[derive(Clone, Debug, PartialEq, CandidType, Deserialize)]
pub struct TransformContext {
    /// The query function reference.
    pub function: TransformFunc,
    /// Opaque context forwarded to the transform on each invocation.
    #[serde(with = "serde_bytes")]
    pub context: Vec<u8>,
}

/// The committee configuration for a flexible outcall
/// (`record { total_requests : nat32; min_responses : nat32; max_responses : nat32 }`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct ReplicationCounts {
    /// Number of nodes asked to perform the outcall (the committee size).
    pub total_requests: u32,
    /// Number of OK responses required before the outcall succeeds.
    pub min_responses: u32,
    /// Maximum number of responses collected.
    pub max_responses: u32,
}

/// Arguments for the `flexible_http_request` management-canister method.
///
/// The Rust field order is irrelevant to the wire format (Candid orders record
/// fields by hashed name); it is kept identical to the upstream struct for ease
/// of comparison.
#[derive(Clone, Debug, PartialEq, CandidType, Deserialize)]
pub struct FlexibleCanisterHttpRequestArgs {
    /// The requested URL.
    pub url: String,
    /// Optional maximum response size in bytes (`None` ⇒ replica default).
    pub max_response_bytes: Option<u64>,
    /// Request headers.
    pub headers: Vec<HttpHeader>,
    /// Optional request body (always `None` for XRC's `GET` ticker outcalls).
    pub body: Option<Vec<u8>>,
    /// The HTTP method.
    pub method: HttpMethod,
    /// Optional per-node transform.
    pub transform: Option<TransformContext>,
    /// Optional committee configuration (`None` ⇒ replica default).
    pub replication: Option<ReplicationCounts>,
}

/// The result of a `flexible_http_request`
/// (`variant { ok : vec http_request_result; err : flexible_http_request_err }`).
#[derive(Clone, Debug, PartialEq, CandidType, Deserialize)]
pub enum FlexibleHttpRequestResult {
    /// The committee returned a vector of per-node responses.
    #[serde(rename = "ok")]
    Ok(Vec<CanisterHttpResponsePayload>),
    /// The outcall failed globally and/or on individual nodes.
    #[serde(rename = "err")]
    Err(FlexibleHttpRequestErr),
}

/// The error payload returned by `flexible_http_request`.
#[derive(Clone, Debug, PartialEq, CandidType, Deserialize)]
pub struct FlexibleHttpRequestErr {
    /// A committee-wide failure, if one occurred. May be `None` when only
    /// individual nodes failed (see `node_details`) without tripping a global
    /// condition.
    pub global_error: Option<FlexibleHttpGlobalError>,
    /// Per-node outcome details (resource usage and any node-local error).
    pub node_details: Vec<FlexibleHttpNodeDetail>,
    /// A human-readable description of the failure.
    pub message: String,
}

/// A committee-wide failure of a flexible outcall. Each variant carries a
/// `reserved` payload upstream, mirrored here.
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum FlexibleHttpGlobalError {
    /// The request parameters were invalid.
    #[serde(rename = "invalid_parameters")]
    InvalidParameters(candid::Reserved),
    /// The outcall timed out.
    #[serde(rename = "timeout")]
    Timeout(candid::Reserved),
    /// The canister had insufficient cycles for the outcall.
    #[serde(rename = "out_of_cycles")]
    OutOfCycles(candid::Reserved),
    /// The responses exceeded the size budget.
    #[serde(rename = "responses_too_large")]
    ResponsesTooLarge(candid::Reserved),
    /// Too many committee nodes rejected the outcall to reach quorum.
    #[serde(rename = "too_many_rejects")]
    TooManyRejects(candid::Reserved),
}

/// Per-node detail in a flexible outcall error.
#[derive(Clone, Debug, PartialEq, CandidType, Deserialize)]
pub struct FlexibleHttpNodeDetail {
    /// The node that produced this detail.
    pub node_id: Principal,
    /// Resource accounting for this node's outcall.
    pub report: HttpRequestResourceReport,
    /// The node-local error, if this node failed.
    pub error: Option<FlexibleHttpNodeError>,
}

/// Per-node resource usage accounting for an HTTP outcall.
#[derive(Clone, Debug, Default, PartialEq, CandidType, Deserialize)]
pub struct HttpRequestResourceReport {
    /// Raw (pre-transform) response bytes.
    pub raw_response_bytes: Option<ResourceUsage<u64>>,
    /// HTTP round-trip time in milliseconds.
    pub http_roundtrip_time_ms: Option<ResourceUsage<u64>>,
    /// Instructions consumed by the transform.
    pub transform_instructions: Option<ResourceUsage<u64>>,
    /// Transformed (post-transform) response bytes.
    pub transformed_response_bytes: Option<ResourceUsage<u64>>,
    /// Cycles consumed by the outcall.
    pub cycles: Option<ResourceUsage<candid::Nat>>,
}

/// Tracks whether a resource was used (with a value) or exceeded its budget.
#[derive(Clone, Debug, PartialEq, CandidType, Deserialize)]
pub enum ResourceUsage<T> {
    /// The resource was used; carries the amount.
    #[serde(rename = "used")]
    Used(T),
    /// The resource exceeded its budget.
    #[serde(rename = "exceeded")]
    Exceeded(candid::Reserved),
}

/// A node-local error (`record { code : text; message : text }`).
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FlexibleHttpNodeError {
    /// A short error code.
    pub code: String,
    /// A human-readable error message.
    pub message: String,
}

// ============================================================================
// XRC-domain error mapping.
// ============================================================================

/// A committee-wide flexible-outcall failure, projected onto a small enum for
/// stable metric labels and matching. Mirrors [`FlexibleHttpGlobalError`] minus
/// its opaque `reserved` payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalErrorKind {
    /// See [`FlexibleHttpGlobalError::InvalidParameters`].
    InvalidParameters,
    /// See [`FlexibleHttpGlobalError::Timeout`].
    Timeout,
    /// See [`FlexibleHttpGlobalError::OutOfCycles`].
    OutOfCycles,
    /// See [`FlexibleHttpGlobalError::ResponsesTooLarge`].
    ResponsesTooLarge,
    /// See [`FlexibleHttpGlobalError::TooManyRejects`].
    TooManyRejects,
}

impl GlobalErrorKind {
    /// A stable, lowercase label suitable for a metric dimension.
    pub fn as_str(self) -> &'static str {
        match self {
            GlobalErrorKind::InvalidParameters => "invalid_parameters",
            GlobalErrorKind::Timeout => "timeout",
            GlobalErrorKind::OutOfCycles => "out_of_cycles",
            GlobalErrorKind::ResponsesTooLarge => "responses_too_large",
            GlobalErrorKind::TooManyRejects => "too_many_rejects",
        }
    }
}

impl From<&FlexibleHttpGlobalError> for GlobalErrorKind {
    fn from(error: &FlexibleHttpGlobalError) -> Self {
        match error {
            FlexibleHttpGlobalError::InvalidParameters(_) => GlobalErrorKind::InvalidParameters,
            FlexibleHttpGlobalError::Timeout(_) => GlobalErrorKind::Timeout,
            FlexibleHttpGlobalError::OutOfCycles(_) => GlobalErrorKind::OutOfCycles,
            FlexibleHttpGlobalError::ResponsesTooLarge(_) => GlobalErrorKind::ResponsesTooLarge,
            FlexibleHttpGlobalError::TooManyRejects(_) => GlobalErrorKind::TooManyRejects,
        }
    }
}

/// The outcome of a flexible outcall once mapped into XRC's domain.
#[derive(Clone, Debug, PartialEq)]
pub enum FlexibleOutcallError {
    /// The `flexible_http_request` call itself was rejected before producing a
    /// result vector (a transport/perform/reject error, not an application-level
    /// error).
    CallRejected(String),
    /// The reply could not be Candid-encoded or -decoded.
    Candid(String),
    /// The replica returned an application-level `Err`. Carries the mapped
    /// committee-wide error kind (if any), the human-readable message, and the
    /// per-node details for downstream metrics.
    Failed {
        /// The committee-wide error kind, if one was reported.
        global_error: Option<GlobalErrorKind>,
        /// The human-readable failure description.
        message: String,
        /// Per-node outcome details.
        node_details: Vec<FlexibleHttpNodeDetail>,
    },
}

/// Maps a decoded [`FlexibleHttpRequestResult`] into the per-node response vector
/// or an [`FlexibleOutcallError::Failed`].
fn map_result(
    result: FlexibleHttpRequestResult,
) -> Result<Vec<CanisterHttpResponsePayload>, FlexibleOutcallError> {
    match result {
        FlexibleHttpRequestResult::Ok(responses) => Ok(responses),
        FlexibleHttpRequestResult::Err(err) => Err(FlexibleOutcallError::Failed {
            global_error: err.global_error.as_ref().map(GlobalErrorKind::from),
            message: err.message,
            node_details: err.node_details,
        }),
    }
}

// ============================================================================
// Request builder.
// ============================================================================

/// Builds and issues a flexible (committee) HTTPS outcall to the management
/// canister, mirroring the classic [`crate::http::CanisterHttpRequest`] builder.
///
/// The committee configuration defaults to `None` (see [`FlexibleHttpRequest::new`])
/// and cycles default to [`default_cycles`].
pub struct FlexibleHttpRequest {
    args: FlexibleCanisterHttpRequestArgs,
    cycles: u128,
}

impl Default for FlexibleHttpRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl FlexibleHttpRequest {
    /// Creates a new `GET` request seeded with the default `User-Agent` header
    /// and the default cycle amount.
    ///
    /// The committee is left unset (`replication: None`), which asks the replica
    /// to default it to the **live** subnet: `total_requests = N`,
    /// `min_responses = ⌊2N/3⌋ + 1`, `max_responses = N`. We prefer this over an
    /// explicit `Some { total_requests: SUBNET_NODE_COUNT, .. }` for robustness:
    /// the replica **rejects** (does not clamp) a `total_requests` exceeding the
    /// live `N`, so a hardcoded `N` would fail every live outcall if the subnet
    /// ever shrank below it. The trade-off is that the quorum is then the
    /// replica's floor-based `⌊2N/3⌋+1` (= 23 at N = 34) rather than our tuned
    /// ceil-based `MIN_COMMITTEE_RESPONSES` (= 24); a caller that wants the tuned
    /// quorum can still opt into [`full_subnet_replication`] via [`Self::replication`].
    pub fn new() -> Self {
        Self {
            cycles: default_cycles(),
            args: FlexibleCanisterHttpRequestArgs {
                url: String::default(),
                max_response_bytes: None,
                headers: vec![HttpHeader {
                    name: "User-Agent".to_string(),
                    value: DEFAULT_USER_AGENT.to_string(),
                }],
                body: None,
                method: HttpMethod::Get,
                transform: None,
                replication: None,
            },
        }
    }

    /// Assigns the URL and sets the `GET` method.
    pub fn get(self, url: &str) -> Self {
        self.url(url).method(HttpMethod::Get)
    }

    /// Sets the HTTP method.
    pub fn method(mut self, method: HttpMethod) -> Self {
        self.args.method = method;
        self
    }

    /// Sets the URL.
    pub fn url(mut self, url: &str) -> Self {
        self.args.url = url.to_string();
        self
    }

    /// Appends request headers.
    pub fn add_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.args
            .headers
            .extend(headers.into_iter().map(|(name, value)| HttpHeader { name, value }));
        self
    }

    /// Overrides the default `User-Agent` header in place (it is a singleton
    /// header), appending one if none is present.
    pub fn user_agent(mut self, user_agent: &str) -> Self {
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

    /// Sets `max_response_bytes`.
    pub fn max_response_bytes(mut self, max_response_bytes: u64) -> Self {
        self.args.max_response_bytes = Some(max_response_bytes);
        self
    }

    /// Sets the per-node transform to a query method on this canister.
    ///
    /// Must be called from within canister execution ([`ic_cdk::api::canister_self`]
    /// is read here); it is not usable off-IC.
    pub fn transform_context(mut self, method: &str, context: Vec<u8>) -> Self {
        self.args.transform = Some(TransformContext {
            function: TransformFunc::new(ic_cdk::api::canister_self(), method.to_string()),
            context,
        });
        self
    }

    /// Sets an explicit committee configuration, overriding the default
    /// (`None` ⇒ replica-defaulted committee; see [`Self::new`]).
    pub fn replication(mut self, replication: ReplicationCounts) -> Self {
        self.args.replication = Some(replication);
        self
    }

    /// Sets the cycles attached to the outcall.
    pub fn cycles(mut self, cycles: u128) -> Self {
        self.cycles = cycles;
        self
    }

    /// Returns a reference to the assembled request arguments (for inspection and
    /// testing).
    pub fn args(&self) -> &FlexibleCanisterHttpRequestArgs {
        &self.args
    }

    /// Issues the flexible outcall and returns the per-node response vector.
    ///
    /// Uses an unbounded-wait call (a guaranteed response), matching the
    /// long-running nature of an HTTPS outcall.
    pub async fn send(self) -> Result<Vec<CanisterHttpResponsePayload>, FlexibleOutcallError> {
        let encoded = candid::encode_one(&self.args)
            .map_err(|error| FlexibleOutcallError::Candid(format!("encoding args: {error}")))?;

        let response =
            ic_cdk::call::Call::unbounded_wait(Principal::management_canister(), FLEXIBLE_HTTP_REQUEST_METHOD)
                .with_raw_args(&encoded)
                .with_cycles(self.cycles)
                .await
                .map_err(|error| FlexibleOutcallError::CallRejected(error.to_string()))?;

        let result: FlexibleHttpRequestResult = candid::decode_one(&response.into_bytes())
            .map_err(|error| FlexibleOutcallError::Candid(format!("decoding result: {error}")))?;

        map_result(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Collects the values of every header matching `name` (case-insensitive).
    fn header_values(request: &FlexibleHttpRequest, name: &str) -> Vec<String> {
        request
            .args
            .headers
            .iter()
            .filter(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.clone())
            .collect()
    }

    /// The committee quorum is `⌈2N/3⌉ + 1`, clamped to `≤ N`.
    #[test]
    fn committee_quorum_computation() {
        // Production subnet: ⌈68/3⌉ + 1 = 23 + 1 = 24.
        assert_eq!(min_committee_responses(34), 24);
        assert_eq!(MIN_COMMITTEE_RESPONSES, 24);
        // A few other sizes.
        assert_eq!(min_committee_responses(13), 10); // ⌈26/3⌉+1 = 9+1
        assert_eq!(min_committee_responses(3), 3); // ⌈6/3⌉+1 = 3 (not clamped)
        // Tiny committees clamp to N rather than exceeding it.
        assert_eq!(min_committee_responses(1), 1);
        assert_eq!(min_committee_responses(2), 2);
    }

    /// The default committee configuration is one request per node with the
    /// production quorum.
    #[test]
    fn full_subnet_replication_defaults() {
        assert_eq!(
            full_subnet_replication(),
            ReplicationCounts {
                total_requests: 34,
                min_responses: 24,
                max_responses: 34,
            }
        );
    }

    /// A new request is a `GET` carrying the default `User-Agent`, no explicit
    /// committee (`replication: None`, replica-defaulted), and no body/transform.
    #[test]
    fn new_request_defaults() {
        let request = FlexibleHttpRequest::new();
        assert_eq!(request.args().method, HttpMethod::Get);
        assert_eq!(
            header_values(&request, "User-Agent"),
            vec![DEFAULT_USER_AGENT.to_string()]
        );
        assert_eq!(request.args().replication, None);
        assert_eq!(request.args().transform, None);
        assert_eq!(request.args().body, None);
    }

    /// `get` sets the URL and method; `max_response_bytes`, `add_headers`,
    /// `user_agent`, and `replication` update the assembled args.
    #[test]
    fn builder_sets_args() {
        let request = FlexibleHttpRequest::new()
            .get("https://example.com/ticker")
            .max_response_bytes(2048)
            .user_agent("curl/8.0")
            .add_headers(vec![("Accept".to_string(), "application/json".to_string())])
            .replication(ReplicationCounts {
                total_requests: 13,
                min_responses: 10,
                max_responses: 13,
            });

        assert_eq!(request.args().url, "https://example.com/ticker");
        assert_eq!(request.args().method, HttpMethod::Get);
        assert_eq!(request.args().max_response_bytes, Some(2048));
        assert_eq!(header_values(&request, "User-Agent"), vec!["curl/8.0".to_string()]);
        assert_eq!(
            header_values(&request, "Accept"),
            vec!["application/json".to_string()]
        );
        assert_eq!(
            request.args().replication,
            Some(ReplicationCounts {
                total_requests: 13,
                min_responses: 10,
                max_responses: 13,
            })
        );
    }

    /// Attaching cycles works and mirrors the classic outcall (`0` on the
    /// non-application-subnet build).
    #[test]
    fn cycles_default_and_override() {
        #[cfg(not(feature = "application-subnet"))]
        assert_eq!(default_cycles(), 0);
        #[cfg(feature = "application-subnet")]
        assert_eq!(default_cycles(), 500_000_000);

        let request = FlexibleHttpRequest::new().cycles(123);
        assert_eq!(request.cycles, 123);
    }

    /// The request args round-trip through Candid unchanged, including a
    /// transform reference (built directly to avoid the on-IC `canister_self`).
    #[test]
    fn args_candid_round_trip() {
        let args = FlexibleCanisterHttpRequestArgs {
            url: "https://example.com/ticker".to_string(),
            max_response_bytes: Some(4096),
            headers: vec![HttpHeader {
                name: "User-Agent".to_string(),
                value: DEFAULT_USER_AGENT.to_string(),
            }],
            body: None,
            method: HttpMethod::Get,
            transform: Some(TransformContext {
                function: TransformFunc::new(
                    Principal::anonymous(),
                    "transform_live_http_response".to_string(),
                ),
                context: vec![1, 2, 3],
            }),
            replication: Some(full_subnet_replication()),
        };

        let encoded = candid::encode_one(&args).expect("args should encode");
        let decoded: FlexibleCanisterHttpRequestArgs =
            candid::decode_one(&encoded).expect("args should decode");
        assert_eq!(decoded, args);
    }

    /// An `Ok` result decodes into the per-node response vector.
    #[test]
    fn result_ok_decodes_to_responses() {
        let responses = vec![
            CanisterHttpResponsePayload {
                status: 200,
                headers: vec![],
                body: b"{\"price\":\"42.0\"}".to_vec(),
            },
            CanisterHttpResponsePayload {
                status: 200,
                headers: vec![],
                body: b"{\"price\":\"42.1\"}".to_vec(),
            },
        ];
        let result = FlexibleHttpRequestResult::Ok(responses.clone());
        let encoded = candid::encode_one(&result).expect("result should encode");
        let decoded: FlexibleHttpRequestResult =
            candid::decode_one(&encoded).expect("result should decode");

        assert_eq!(map_result(decoded), Ok(responses));
    }

    /// Each global-error variant decodes and maps to its [`GlobalErrorKind`],
    /// preserving the message and per-node details.
    #[test]
    fn result_err_maps_global_errors() {
        let cases = [
            (
                FlexibleHttpGlobalError::TooManyRejects(candid::Reserved),
                GlobalErrorKind::TooManyRejects,
            ),
            (
                FlexibleHttpGlobalError::Timeout(candid::Reserved),
                GlobalErrorKind::Timeout,
            ),
            (
                FlexibleHttpGlobalError::OutOfCycles(candid::Reserved),
                GlobalErrorKind::OutOfCycles,
            ),
            (
                FlexibleHttpGlobalError::ResponsesTooLarge(candid::Reserved),
                GlobalErrorKind::ResponsesTooLarge,
            ),
            (
                FlexibleHttpGlobalError::InvalidParameters(candid::Reserved),
                GlobalErrorKind::InvalidParameters,
            ),
        ];

        for (global_error, expected_kind) in cases {
            let err = FlexibleHttpRequestErr {
                global_error: Some(global_error),
                node_details: vec![FlexibleHttpNodeDetail {
                    node_id: Principal::anonymous(),
                    report: HttpRequestResourceReport::default(),
                    error: Some(FlexibleHttpNodeError {
                        code: "429".to_string(),
                        message: "rate limited".to_string(),
                    }),
                }],
                message: "committee failure".to_string(),
            };
            let result = FlexibleHttpRequestResult::Err(err.clone());
            let encoded = candid::encode_one(&result).expect("err result should encode");
            let decoded: FlexibleHttpRequestResult =
                candid::decode_one(&encoded).expect("err result should decode");

            match map_result(decoded) {
                Err(FlexibleOutcallError::Failed {
                    global_error,
                    message,
                    node_details,
                }) => {
                    assert_eq!(global_error, Some(expected_kind));
                    assert_eq!(message, "committee failure");
                    assert_eq!(node_details, err.node_details);
                }
                other => panic!("expected Failed, got {other:?}"),
            }
        }
    }

    /// An `Err` with no committee-wide error (only per-node failures) maps to
    /// `Failed` with `global_error == None`.
    #[test]
    fn result_err_without_global_error() {
        let err = FlexibleHttpRequestErr {
            global_error: None,
            node_details: vec![],
            message: "minority of nodes failed".to_string(),
        };
        let result = FlexibleHttpRequestResult::Err(err);
        let encoded = candid::encode_one(&result).expect("err result should encode");
        let decoded: FlexibleHttpRequestResult =
            candid::decode_one(&encoded).expect("err result should decode");

        match map_result(decoded) {
            Err(FlexibleOutcallError::Failed { global_error, .. }) => {
                assert_eq!(global_error, None);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// The metric labels are stable and lowercase.
    #[test]
    fn global_error_kind_labels() {
        assert_eq!(GlobalErrorKind::TooManyRejects.as_str(), "too_many_rejects");
        assert_eq!(GlobalErrorKind::Timeout.as_str(), "timeout");
        assert_eq!(GlobalErrorKind::OutOfCycles.as_str(), "out_of_cycles");
        assert_eq!(
            GlobalErrorKind::ResponsesTooLarge.as_str(),
            "responses_too_large"
        );
        assert_eq!(
            GlobalErrorKind::InvalidParameters.as_str(),
            "invalid_parameters"
        );
    }
}
