use std::borrow::Cow;
use candid::{decode_one, encode_one, CandidType, Deserialize, Nat, Principal};
use ic_cdk::call::{CallFailed, CandidDecodeFailed};
use ic_stable_structures::Storable;
use ic_xrc_types::{ExchangeRate, ExchangeRateError, GetExchangeRateRequest};
use num_traits::ToPrimitive;

#[derive(CandidType, Deserialize)]
pub struct Config {
    pub xrc_canister_id: Principal,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            xrc_canister_id: Principal::anonymous(),
        }
    }
}

impl Storable for Config {
    fn to_bytes(&self) -> std::borrow::Cow<'_, [u8]> {
        let bytes = encode_one(self).expect("failed to encode config");
        Cow::Owned(bytes)
    }

    fn from_bytes(bytes: Vec<u8>) -> Self {
        decode_one(&bytes).expect("failed to decode config")
    }
}

#[derive(CandidType, Deserialize)]
pub enum EntryResult {
    Rate(ExchangeRate),
    RateError(ExchangeRateError),
    CallError(CallError),
}

#[derive(CandidType, Deserialize)]
pub struct Entry {
    pub request: GetExchangeRateRequest,
    pub result: EntryResult,
}

/// Reject code of an inter-canister call, mirroring the variants the monitor
/// has always persisted. Vendored locally rather than re-using the CDK's type
/// so the stored entries and the candid interface stay byte-identical now that
/// the CDK no longer exposes its own reject-code type. (Candid compatibility is
/// determined by variant names/shape; the numeric discriminants are just a convenience.)
#[derive(CandidType, Deserialize, Clone, Copy, Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RejectionCode {
    NoError = 0,
    SysFatal = 1,
    SysTransient = 2,
    DestinationInvalid = 3,
    CanisterReject = 4,
    CanisterError = 5,
    Unknown,
}

impl RejectionCode {
    /// Maps a raw IC reject code to the corresponding variant, matching the
    /// conversion the CDK used to perform; codes outside the known range
    /// collapse to [`RejectionCode::Unknown`].
    fn from_raw(code: u32) -> Self {
        match code {
            0 => RejectionCode::NoError,
            1 => RejectionCode::SysFatal,
            2 => RejectionCode::SysTransient,
            3 => RejectionCode::DestinationInvalid,
            4 => RejectionCode::CanisterReject,
            5 => RejectionCode::CanisterError,
            _ => RejectionCode::Unknown,
        }
    }
}

#[derive(CandidType, Clone, Deserialize)]
pub struct CallError {
    pub rejection_code: RejectionCode,
    pub err: String,
}

impl From<CallFailed> for CallError {
    fn from(error: CallFailed) -> Self {
        match error {
            CallFailed::CallRejected(rejected) => CallError {
                rejection_code: RejectionCode::from_raw(rejected.raw_reject_code()),
                err: rejected.reject_message().to_string(),
            },
            // The call never reached the callee: the message could not be
            // enqueued. Surface it as a transient system error.
            CallFailed::CallPerformFailed(error) => CallError {
                rejection_code: RejectionCode::SysTransient,
                err: error.to_string(),
            },
            // We lacked the cycles to perform the call ourselves.
            CallFailed::InsufficientLiquidCycleBalance(error) => CallError {
                rejection_code: RejectionCode::CanisterError,
                err: error.to_string(),
            },
        }
    }
}

impl From<CandidDecodeFailed> for CallError {
    fn from(error: CandidDecodeFailed) -> Self {
        // The callee replied, but its payload could not be decoded.
        CallError {
            rejection_code: RejectionCode::CanisterError,
            err: error.to_string(),
        }
    }
}

#[derive(CandidType, Deserialize)]
pub struct GetEntriesRequest {
    pub offset: Nat,
    pub limit: Option<Nat>,
}

impl GetEntriesRequest {
    pub fn offset_and_limit(&self) -> Result<(usize, usize), String> {
        let offset = self.offset.0.to_usize().ok_or_else(|| {
            format!(
                "offset {} is too large, max allowed: {}",
                self.offset,
                u64::MAX
            )
        })?;

        let limit = match &self.limit {
            Some(limit) => limit.0.to_usize().unwrap_or(100),
            None => 100,
        };

        Ok((offset, limit))
    }
}

#[derive(CandidType, Deserialize)]
pub struct GetEntriesResponse {
    pub entries: Vec<Entry>,
    pub total: Nat,
}
