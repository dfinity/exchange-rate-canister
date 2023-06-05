use crate::ExtractError;

use super::{extract_rate, Binance, ExtractedValue, IsExchange};

type BinanceResponse = Vec<(
    u64,
    String,
    String,
    String,
    String,
    String,
    u64,
    String,
    u64,
    String,
    String,
    String,
)>;

impl IsExchange for Binance {
    fn get_base_url(&self) -> &str {
        "https://api.binance.com/api/v3/klines?symbol=BASE_ASSETQUOTE_ASSET&interval=1m&startTime=START_TIME&endTime=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: BinanceResponse| {
            response
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{exchanges::Binance, utils::test::load_file, Exchange, DAI, ONE_KIB, USDC, USDT};

    #[test]
    fn to_string() {
        let exchange = Exchange::Binance(Binance);
        assert_eq!(exchange.to_string(), "Binance");
    }

    #[test]
    fn get_base_url() {
        let timestamp = 1661524016;
        let binance = Binance;
        let query_string = binance.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.binance.com/api/v3/klines?symbol=BTCICP&interval=1m&startTime=1661523960000&endTime=1661523960000");
    }

    #[test]
    fn ipv6_support() {
        let binance = Binance;
        assert!(!binance.supports_ipv6());
    }

    #[test]
    fn supported_stablecoin_pairs() {
        let binance = Binance;
        assert_eq!(
            binance.supported_stablecoin_pairs(),
            &[(DAI, USDT), (USDC, USDT)]
        );
    }

    #[test]
    fn extract_rate_from_binance() {
        let binance = Binance;
        let query_response = load_file("test-data/exchanges/binance.json");
        let extracted_rate = binance.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 41_960_000_000));
    }

    #[test]
    fn max_response_bytes() {
        let exchange = Exchange::Binance(Binance);
        assert_eq!(exchange.max_response_bytes(), ONE_KIB);
    }
}
