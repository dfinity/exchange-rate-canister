use std::borrow::Cow;

use candid::{decode_one, encode_one, CandidType, Deserialize, Nat, Principal};
use ic_cdk::api::call::RejectionCode;
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
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
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

#[derive(CandidType, Clone, Deserialize)]
pub struct CallError {
    pub rejection_code: RejectionCode,
    pub err: String,
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
