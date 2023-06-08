use xrc::{Forex, FOREX_SOURCES};

use crate::container::ExchangeResponse;

mod australia;
mod bosnia;
mod canada;
mod europe;
mod georgia;
mod italy;
mod myanmar;
mod nepal;
mod singapore;
mod switzerland;
mod uzbekistan;

const ONE_DAY_SECONDS: u64 = 86_400;

pub fn build_responses(now_timestamp: u64) -> impl Iterator<Item = ExchangeResponse> + 'static {
    // Forex sources go back one day.
    let yesterday_timestamp = (now_timestamp.saturating_sub(ONE_DAY_SECONDS) / ONE_DAY_SECONDS)
        .saturating_mul(ONE_DAY_SECONDS);
    FOREX_SOURCES.iter().map(move |forex| {
        let url = forex.get_url(yesterday_timestamp);
        let body = match forex {
            Forex::MonetaryAuthorityOfSingapore(_) => {
                singapore::build_response_body(yesterday_timestamp)
            }
            Forex::CentralBankOfMyanmar(_) => myanmar::build_response_body(yesterday_timestamp),
            Forex::CentralBankOfBosniaHerzegovina(_) => {
                bosnia::build_response_body(yesterday_timestamp)
            }
            Forex::EuropeanCentralBank(_) => europe::build_response_body(yesterday_timestamp),
            Forex::BankOfCanada(_) => canada::build_response_body(yesterday_timestamp),
            Forex::CentralBankOfUzbekistan(_) => {
                uzbekistan::build_response_body(yesterday_timestamp)
            }
            Forex::ReserveBankOfAustralia(_) => australia::build_response_body(yesterday_timestamp),
            Forex::CentralBankOfNepal(_) => nepal::build_response_body(yesterday_timestamp),
            Forex::CentralBankOfGeorgia(_) => georgia::build_response_body(yesterday_timestamp),
            Forex::BankOfItaly(_) => italy::build_response_body(yesterday_timestamp),
            Forex::SwissFederalOfficeForCustoms(_) => {
                switzerland::build_response_body(yesterday_timestamp)
            }
        };
        ExchangeResponse::builder()
            .name(forex.to_string())
            .url(url)
            .body(body)
            .build()
    })
}
