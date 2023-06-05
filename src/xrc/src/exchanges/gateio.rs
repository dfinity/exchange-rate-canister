use super::{extract_rate, ExtractedValue, GateIo, IsExchange};
use crate::{ExtractError, DAI, USDT};

type GateIoResponse = Vec<(String, String, String, String, String, String, String)>;

impl IsExchange for GateIo {
    fn get_base_url(&self) -> &str {
        "https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=BASE_ASSET_QUOTE_ASSET&interval=1m&from=START_TIME&to=END_TIME"
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: GateIoResponse| {
            response
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.3.clone()))
        })
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(DAI, USDT)]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{exchanges::GateIo, utils::test::load_file, Exchange, ONE_KIB, USDT};

    #[test]
    fn to_string() {
        let exchange = Exchange::GateIo(GateIo);
        assert_eq!(exchange.to_string(), "GateIo");
    }

    #[test]
    fn get_base_url() {
        let timestamp = 1661524016;
        let gateio = GateIo;
        let query_string = gateio.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=BTC_ICP&interval=1m&from=1661523960&to=1661523960");
    }

    #[test]
    fn ipv6_support() {
        let gateio = GateIo;
        assert!(!gateio.supports_ipv6());
    }

    #[test]
    fn supported_stablecoin_pairs() {
        let gate_io = GateIo;
        assert_eq!(gate_io.supported_stablecoin_pairs(), &[(DAI, USDT)]);
    }

    #[test]
    fn extract_rate_from_gateio() {
        let gateio = GateIo;
        let query_response = load_file("test-data/exchanges/gateio.json");
        let extracted_rate = gateio.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 42_640_000_000));
    }

    #[test]
    fn max_response_bytes() {
        let exchange = Exchange::GateIo(GateIo);
        assert_eq!(exchange.max_response_bytes(), ONE_KIB);
    }
}
