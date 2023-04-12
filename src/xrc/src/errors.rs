use ic_xrc_types::{ExchangeRateError, OtherError};

pub(crate) const TIMESTAMP_IS_IN_FUTURE_ERROR_CODE: u32 = 1;

pub(crate) const INVALID_RATES_RECEIVED_ERROR_CODE: u32 = 4;

pub(crate) const INVALID_RATES_RECEIVED_ERROR_MESSAGE: &str = "Invalid rates received";

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
