use ic_xrc_types::Asset;

use crate::{api::usd_asset, ExtractError, USDC, USDT};

use super::{extract_rate, Coinbase, ExtractedValue, IsExchange};

type CoinbaseResponse = Vec<(u64, f64, f64, f64, f64, f64)>;

impl IsExchange for Coinbase {
    fn get_base_url(&self) -> &str {
        "https://api.pro.coinbase.com/products/BASE_ASSET-QUOTE_ASSET/candles?granularity=60&start=START_TIME&end=END_TIME"
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: CoinbaseResponse| {
            response.first().map(|kline| ExtractedValue::Float(kline.3))
        })
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDT, USDC)]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{exchanges::Coinbase, utils::test::load_file, Exchange, ONE_KIB, USDC, USDT};

    #[test]
    fn to_string() {
        let exchange = Exchange::Coinbase(Coinbase);
        assert_eq!(exchange.to_string(), "Coinbase");
    }

    #[test]
    fn get_base_url() {
        let timestamp = 1661524016;
        let coinbase = Coinbase;
        let query_string = coinbase.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.pro.coinbase.com/products/BTC-ICP/candles?granularity=60&start=1661523960&end=1661523960");
    }

    #[test]
    fn ipv6_support() {
        let coinbase = Coinbase;
        assert!(coinbase.supports_ipv6());
    }

    #[test]
    fn supported_stablecoin_pairs() {
        let coinbase = Coinbase;
        assert_eq!(coinbase.supported_stablecoin_pairs(), &[(USDT, USDC)]);
    }

    #[test]
    fn extract_rate_from_coinbase() {
        let coinbase = Coinbase;
        let query_response = load_file("test-data/exchanges/coinbase.json");
        let extracted_rate = coinbase.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 49_180_000_000));
    }

    #[test]
    fn max_response_bytes() {
        let exchange = Exchange::Coinbase(Coinbase);
        assert_eq!(exchange.max_response_bytes(), ONE_KIB);
    }
}
