use ic_xrc_types::{ExchangeRateError, OtherError};

pub(crate) const TIMESTAMP_IS_IN_FUTURE_ERROR_CODE: u32 = 1;
pub(crate) const BASE_ASSET_INVALID_SYMBOL_ERROR_CODE: u32 = 2;
pub(crate) const QUOTE_ASSET_INVALID_SYMBOL_ERROR_CODE: u32 = 3;
pub(crate) const INVALID_RATE_ERROR_CODE: u32 = 4;

pub(crate) const BASE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE: &str = "Base asset symbol is invalid";
pub(crate) const QUOTE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE: &str = "Quote asset symbol is invalid";
pub(crate) const INVALID_RATE_ERROR_MESSAGE: &str = "The computed rate is invalid";

pub(crate) fn timestamp_is_in_future_error(
    requested_timestamp: u64,
    current_timestamp: u64,
) -> ExchangeRateError {
    ExchangeRateError::Other(OtherError {
        code: TIMESTAMP_IS_IN_FUTURE_ERROR_CODE,
        description: format!(
            "Current IC time is {}. {} is in the future!",
            current_timestamp, requested_timestamp
        ),
    })
}

pub(crate) fn base_asset_symbol_invalid_error() -> ExchangeRateError {
    ExchangeRateError::Other(OtherError {
        code: BASE_ASSET_INVALID_SYMBOL_ERROR_CODE,
        description: BASE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE.to_string(),
    })
}

pub(crate) fn quote_asset_symbol_invalid_error() -> ExchangeRateError {
    ExchangeRateError::Other(OtherError {
        code: QUOTE_ASSET_INVALID_SYMBOL_ERROR_CODE,
        description: QUOTE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE.to_string(),
    })
}
