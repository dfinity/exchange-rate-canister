use serde::Serialize;

/// The types defined here are solely used to serialize the data structures to JSON.

#[derive(Serialize)]
pub struct Entry {
    pub request: GetExchangeRateRequest,
    pub result: EntryResult,
}

impl From<monitor_canister::types::Entry> for Entry {
    fn from(entry: monitor_canister::types::Entry) -> Self {
        Self {
            request: entry.request.into(),
            result: entry.result.into(),
        }
    }
}

#[derive(Serialize)]
pub struct GetExchangeRateRequest {
    pub base_asset: Asset,
    pub quote_asset: Asset,
    pub timestamp: Option<u64>,
}

impl From<xrc::candid::GetExchangeRateRequest> for GetExchangeRateRequest {
    fn from(request: xrc::candid::GetExchangeRateRequest) -> Self {
        Self {
            base_asset: request.base_asset.into(),
            quote_asset: request.quote_asset.into(),
            timestamp: request.timestamp,
        }
    }
}

#[derive(Serialize)]
pub enum RejectionCode {
    NoError,
    SysFatal,
    SysTransient,
    DestinationInvalid,
    CanisterReject,
    CanisterError,
    Unknown,
}

impl From<monitor_canister::types::RejectionCode> for RejectionCode {
    fn from(code: monitor_canister::types::RejectionCode) -> Self {
        match code {
            monitor_canister::types::RejectionCode::NoError => RejectionCode::NoError,
            monitor_canister::types::RejectionCode::SysFatal => RejectionCode::SysFatal,
            monitor_canister::types::RejectionCode::SysTransient => RejectionCode::SysTransient,
            monitor_canister::types::RejectionCode::DestinationInvalid => {
                RejectionCode::DestinationInvalid
            }
            monitor_canister::types::RejectionCode::CanisterReject => RejectionCode::CanisterReject,
            monitor_canister::types::RejectionCode::CanisterError => RejectionCode::CanisterError,
            monitor_canister::types::RejectionCode::Unknown => RejectionCode::Unknown,
        }
    }
}

#[derive(Serialize)]
pub struct CallError {
    pub rejection_code: RejectionCode,
    pub err: String,
}

impl From<monitor_canister::types::CallError> for CallError {
    fn from(err: monitor_canister::types::CallError) -> Self {
        Self {
            rejection_code: err.rejection_code.into(),
            err: err.err,
        }
    }
}

#[derive(Serialize)]
pub enum EntryResult {
    Rate(ExchangeRate),
    RateError(ExchangeRateError),
    CallError(CallError),
}

impl From<monitor_canister::types::EntryResult> for EntryResult {
    fn from(result: monitor_canister::types::EntryResult) -> Self {
        match result {
            monitor_canister::types::EntryResult::Rate(rate) => EntryResult::Rate(rate.into()),
            monitor_canister::types::EntryResult::RateError(err) => {
                EntryResult::RateError(err.into())
            }
            monitor_canister::types::EntryResult::CallError(err) => {
                EntryResult::CallError(err.into())
            }
        }
    }
}

#[derive(Serialize)]
pub struct ExchangeRate {
    pub base_asset: Asset,
    pub quote_asset: Asset,
    pub timestamp: u64,
    pub rate: u64,
    pub metadata: ExchangeRateMetadata,
}

impl From<xrc::candid::ExchangeRate> for ExchangeRate {
    fn from(rate: xrc::candid::ExchangeRate) -> Self {
        Self {
            base_asset: rate.base_asset.into(),
            quote_asset: rate.quote_asset.into(),
            timestamp: rate.timestamp,
            rate: rate.rate,
            metadata: rate.metadata.into(),
        }
    }
}

#[derive(Serialize)]
pub struct ExchangeRateMetadata {
    pub decimals: u32,
    pub base_asset_num_queried_sources: usize,
    pub base_asset_num_received_rates: usize,
    pub quote_asset_num_queried_sources: usize,
    pub quote_asset_num_received_rates: usize,
    pub standard_deviation: u64,
    pub forex_timestamp: Option<u64>,
}

impl From<xrc::candid::ExchangeRateMetadata> for ExchangeRateMetadata {
    fn from(metadata: xrc::candid::ExchangeRateMetadata) -> Self {
        Self {
            decimals: metadata.decimals,
            base_asset_num_queried_sources: metadata.base_asset_num_queried_sources,
            base_asset_num_received_rates: metadata.quote_asset_num_received_rates,
            quote_asset_num_queried_sources: metadata.quote_asset_num_queried_sources,
            quote_asset_num_received_rates: metadata.quote_asset_num_received_rates,
            standard_deviation: metadata.standard_deviation,
            forex_timestamp: metadata.forex_timestamp,
        }
    }
}

#[derive(Serialize)]
pub enum AssetClass {
    Cryptocurrency,
    FiatCurrency,
}

impl From<xrc::candid::AssetClass> for AssetClass {
    fn from(cls: xrc::candid::AssetClass) -> Self {
        match cls {
            xrc::candid::AssetClass::Cryptocurrency => AssetClass::Cryptocurrency,
            xrc::candid::AssetClass::FiatCurrency => AssetClass::FiatCurrency,
        }
    }
}

/// Exchange rates are derived for pairs of assets captured in this struct.
#[derive(Serialize)]
pub struct Asset {
    pub symbol: String,
    pub class: AssetClass,
}

impl From<xrc::candid::Asset> for Asset {
    fn from(asset: xrc::candid::Asset) -> Self {
        Self {
            symbol: asset.symbol,
            class: asset.class.into(),
        }
    }
}

#[derive(Serialize)]
pub enum ExchangeRateError {
    AnonymousPrincipalNotAllowed,
    Pending,
    CryptoBaseAssetNotFound,
    CryptoQuoteAssetNotFound,
    StablecoinRateNotFound,
    StablecoinRateTooFewRates,
    StablecoinRateZeroRate,
    ForexInvalidTimestamp,
    ForexBaseAssetNotFound,
    ForexQuoteAssetNotFound,
    ForexAssetsNotFound,
    RateLimited,
    NotEnoughCycles,
    InconsistentRatesReceived,
    Other(OtherError),
}

impl From<xrc::candid::ExchangeRateError> for ExchangeRateError {
    fn from(err: xrc::candid::ExchangeRateError) -> Self {
        match err {
            xrc::candid::ExchangeRateError::Pending => ExchangeRateError::Pending,
            xrc::candid::ExchangeRateError::AnonymousPrincipalNotAllowed => {
                ExchangeRateError::AnonymousPrincipalNotAllowed
            }
            xrc::candid::ExchangeRateError::CryptoBaseAssetNotFound => {
                ExchangeRateError::CryptoBaseAssetNotFound
            }
            xrc::candid::ExchangeRateError::CryptoQuoteAssetNotFound => {
                ExchangeRateError::CryptoQuoteAssetNotFound
            }
            xrc::candid::ExchangeRateError::StablecoinRateNotFound => {
                ExchangeRateError::StablecoinRateNotFound
            }
            xrc::candid::ExchangeRateError::StablecoinRateTooFewRates => {
                ExchangeRateError::StablecoinRateTooFewRates
            }
            xrc::candid::ExchangeRateError::StablecoinRateZeroRate => {
                ExchangeRateError::StablecoinRateZeroRate
            }
            xrc::candid::ExchangeRateError::ForexInvalidTimestamp => {
                ExchangeRateError::ForexInvalidTimestamp
            }
            xrc::candid::ExchangeRateError::ForexBaseAssetNotFound => {
                ExchangeRateError::ForexBaseAssetNotFound
            }
            xrc::candid::ExchangeRateError::ForexQuoteAssetNotFound => {
                ExchangeRateError::ForexQuoteAssetNotFound
            }
            xrc::candid::ExchangeRateError::ForexAssetsNotFound => {
                ExchangeRateError::ForexAssetsNotFound
            }
            xrc::candid::ExchangeRateError::RateLimited => ExchangeRateError::RateLimited,
            xrc::candid::ExchangeRateError::NotEnoughCycles => ExchangeRateError::NotEnoughCycles,
            xrc::candid::ExchangeRateError::InconsistentRatesReceived => {
                ExchangeRateError::InconsistentRatesReceived
            }
            xrc::candid::ExchangeRateError::Other(err) => ExchangeRateError::Other(err.into()),
        }
    }
}

#[derive(Serialize)]
pub struct OtherError {
    pub code: u32,
    pub description: String,
}

impl From<xrc::candid::OtherError> for OtherError {
    fn from(err: xrc::candid::OtherError) -> Self {
        Self {
            code: err.code,
            description: err.description,
        }
    }
}
