use serde::Deserialize;

use super::{extract_rate, ExtractedValue, IsExchange, Mexc};
use crate::ExtractError;

#[derive(Deserialize)]
struct MexcResponse {
    data: Vec<(u64, String, String, String, String, String, String)>,
}

impl IsExchange for Mexc {
    fn get_base_url(&self) -> &str {
        "https://www.mexc.com/open/api/v2/market/kline?symbol=BASE_ASSET_QUOTE_ASSET&interval=1m&start_time=START_TIME&limit=1"
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: MexcResponse| {
            response
                .data
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }
}
