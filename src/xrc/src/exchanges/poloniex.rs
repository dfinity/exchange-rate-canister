use super::{extract_rate, ExtractedValue, IsExchange, Poloniex};
use crate::{ExtractError, DAI, USDC, USDT};

#[allow(clippy::type_complexity)]
type PoloniexResponse = Vec<(
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    String,
    String,
    u64,
    u64,
)>;

impl IsExchange for Poloniex {
    fn get_base_url(&self) -> &str {
        "https://api.poloniex.com/markets/BASE_ASSET_QUOTE_ASSET/candles?interval=MINUTE_1&startTime=START_TIME&endTime=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and add 1 millisecond.
        timestamp.saturating_mul(1000).saturating_add(1).to_string()
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(DAI, USDT), (USDT, USDC)]
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: PoloniexResponse| {
            response
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.2.clone()))
        })
    }
}
