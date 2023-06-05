use serde::Deserialize;

use crate::{ExtractError, DAI, ONE_KIB, USDC, USDT};

use super::{extract_rate, ExtractedValue, IsExchange, KuCoin};

#[derive(Deserialize)]
struct KuCoinResponse {
    data: Vec<(String, String, String, String, String, String, String)>,
}

impl IsExchange for KuCoin {
    fn get_base_url(&self) -> &str {
        "https://api.kucoin.com/api/v1/market/candles?symbol=BASE_ASSET-QUOTE_ASSET&type=1min&startAt=START_TIME&endAt=END_TIME"
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // In order to include the end time, a second must be added.
        timestamp.saturating_add(1).to_string()
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: KuCoinResponse| {
            response
                .data
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDC, USDT), (USDT, DAI)]
    }

    fn max_response_bytes(&self) -> u64 {
        2 * ONE_KIB
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{exchanges::KuCoin, utils::test::load_file, Exchange};

    #[test]
    fn to_string() {
        let exchange = Exchange::KuCoin(KuCoin);
        assert_eq!(exchange.to_string(), "KuCoin");
    }

    #[test]
    fn get_base_url() {
        let timestamp = 1661524016;
        let kucoin = KuCoin;
        let query_string = kucoin.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.kucoin.com/api/v1/market/candles?symbol=BTC-ICP&type=1min&startAt=1661523960&endAt=1661523961");
    }

    #[test]
    fn ipv6_support() {
        let kucoin = KuCoin;
        assert!(kucoin.supports_ipv6());
    }

    #[test]
    fn supported_stablecoin_pairs() {
        let kucoin = KuCoin;
        assert_eq!(
            kucoin.supported_stablecoin_pairs(),
            &[(USDC, USDT), (USDT, DAI)]
        );
    }

    #[test]
    fn extract_rate_from_kucoin() {
        let kucoin = KuCoin;
        let query_response = load_file("test-data/exchanges/kucoin.json");
        let extracted_rate = kucoin.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 345_426_000_000));
    }

    #[test]
    fn max_response_bytes() {
        let exchange = Exchange::KuCoin(KuCoin);
        assert_eq!(exchange.max_response_bytes(), 2 * ONE_KIB);
    }
}
