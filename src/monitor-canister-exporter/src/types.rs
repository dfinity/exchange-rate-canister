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

impl From<ic_xrc_types::GetExchangeRateRequest> for GetExchangeRateRequest {
    fn from(request: ic_xrc_types::GetExchangeRateRequest) -> Self {
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

impl From<ic_xrc_types::ExchangeRate> for ExchangeRate {
    fn from(rate: ic_xrc_types::ExchangeRate) -> Self {
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

impl From<ic_xrc_types::ExchangeRateMetadata> for ExchangeRateMetadata {
    fn from(metadata: ic_xrc_types::ExchangeRateMetadata) -> Self {
        Self {
            decimals: metadata.decimals,
            base_asset_num_queried_sources: metadata.base_asset_num_queried_sources,
            base_asset_num_received_rates: metadata.base_asset_num_received_rates,
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

impl From<ic_xrc_types::AssetClass> for AssetClass {
    fn from(cls: ic_xrc_types::AssetClass) -> Self {
        match cls {
            ic_xrc_types::AssetClass::Cryptocurrency => AssetClass::Cryptocurrency,
            ic_xrc_types::AssetClass::FiatCurrency => AssetClass::FiatCurrency,
        }
    }
}

/// Exchange rates are derived for pairs of assets captured in this struct.
#[derive(Serialize)]
pub struct Asset {
    pub symbol: String,
    pub class: AssetClass,
}

impl From<ic_xrc_types::Asset> for Asset {
    fn from(asset: ic_xrc_types::Asset) -> Self {
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

impl From<ic_xrc_types::ExchangeRateError> for ExchangeRateError {
    fn from(err: ic_xrc_types::ExchangeRateError) -> Self {
        match err {
            ic_xrc_types::ExchangeRateError::Pending => ExchangeRateError::Pending,
            ic_xrc_types::ExchangeRateError::AnonymousPrincipalNotAllowed => {
                ExchangeRateError::AnonymousPrincipalNotAllowed
            }
            ic_xrc_types::ExchangeRateError::CryptoBaseAssetNotFound => {
                ExchangeRateError::CryptoBaseAssetNotFound
            }
            ic_xrc_types::ExchangeRateError::CryptoQuoteAssetNotFound => {
                ExchangeRateError::CryptoQuoteAssetNotFound
            }
            ic_xrc_types::ExchangeRateError::StablecoinRateNotFound => {
                ExchangeRateError::StablecoinRateNotFound
            }
            ic_xrc_types::ExchangeRateError::StablecoinRateTooFewRates => {
                ExchangeRateError::StablecoinRateTooFewRates
            }
            ic_xrc_types::ExchangeRateError::StablecoinRateZeroRate => {
                ExchangeRateError::StablecoinRateZeroRate
            }
            ic_xrc_types::ExchangeRateError::ForexInvalidTimestamp => {
                ExchangeRateError::ForexInvalidTimestamp
            }
            ic_xrc_types::ExchangeRateError::ForexBaseAssetNotFound => {
                ExchangeRateError::ForexBaseAssetNotFound
            }
            ic_xrc_types::ExchangeRateError::ForexQuoteAssetNotFound => {
                ExchangeRateError::ForexQuoteAssetNotFound
            }
            ic_xrc_types::ExchangeRateError::ForexAssetsNotFound => {
                ExchangeRateError::ForexAssetsNotFound
            }
            ic_xrc_types::ExchangeRateError::RateLimited => ExchangeRateError::RateLimited,
            ic_xrc_types::ExchangeRateError::NotEnoughCycles => ExchangeRateError::NotEnoughCycles,
            ic_xrc_types::ExchangeRateError::InconsistentRatesReceived => {
                ExchangeRateError::InconsistentRatesReceived
            }
            ic_xrc_types::ExchangeRateError::Other(err) => ExchangeRateError::Other(err.into()),
        }
    }
}

#[derive(Serialize)]
pub struct OtherError {
    pub code: u32,
    pub description: String,
}

impl From<ic_xrc_types::OtherError> for OtherError {
    fn from(err: ic_xrc_types::OtherError) -> Self {
        Self {
            code: err.code,
            description: err.description,
        }
    }
}
