use serde::Deserialize;

use crate::{ExtractError, ONE_KIB};

use super::{extract_rate, ExtractedValue, IsExchange, Okx};

type OkxResponseDataEntry = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

#[derive(Deserialize)]
struct OkxResponse {
    data: Vec<OkxResponseDataEntry>,
}

impl IsExchange for Okx {
    fn get_base_url(&self) -> &str {
        // Counterintuitively, "after" specifies the end time, and "before" specifies the start time.
        "https://www.okx.com/api/v5/market/history-candles?instId=BASE_ASSET-QUOTE_ASSET&bar=1m&before=START_TIME&after=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and subtract 1 minute and 1 millisecond.
        // A minute is subtracted because OKX does not return rates for the current minute.
        // Subtracting a minute does not invalidate results when the request contains a timestamp
        // in the past because the most recent candle data is always at index 0.
        timestamp
            .saturating_mul(1000)
            .saturating_sub(60_001)
            .to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and add 1 millisecond.
        timestamp.saturating_mul(1000).saturating_add(1).to_string()
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: OkxResponse| {
            response
                .data
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn max_response_bytes(&self) -> u64 {
        2 * ONE_KIB
    }
}
