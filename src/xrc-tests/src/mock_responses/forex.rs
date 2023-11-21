use std::collections::HashMap;

use xrc::{Forex, FOREX_SOURCES};

use crate::{container::ExchangeResponse, ONE_DAY_SECONDS};

mod australia;
mod bosnia;
mod canada;
mod europe;
mod georgia;
mod italy;
mod myanmar;
mod nepal;
mod switzerland;
mod uzbekistan;

pub fn build_common_responses(
    now_timestamp: u64,
) -> impl Iterator<Item = ExchangeResponse> + 'static {
    build_responses(now_timestamp, |_| Some(HashMap::new()))
}

pub fn build_responses<F>(
    now_timestamp: u64,
    rate_lookup: F,
) -> impl Iterator<Item = ExchangeResponse> + 'static
where
    F: Fn(&Forex) -> Option<HashMap<&str, &str>> + 'static,
{
    // Forex sources go back one day.
    let yesterday_timestamp = now_timestamp
        .saturating_sub(ONE_DAY_SECONDS)
        .saturating_div(ONE_DAY_SECONDS)
        .saturating_mul(ONE_DAY_SECONDS);
    FOREX_SOURCES.iter().map(move |forex| {
        let url = forex.get_url(yesterday_timestamp);
        let body = rate_lookup(forex)
            .map(|rates| match forex {
                Forex::CentralBankOfMyanmar(_) => {
                    myanmar::build_response_body(yesterday_timestamp, rates)
                }
                Forex::CentralBankOfBosniaHerzegovina(_) => {
                    bosnia::build_response_body(yesterday_timestamp, rates)
                }
                Forex::EuropeanCentralBank(_) => {
                    europe::build_response_body(yesterday_timestamp, rates)
                }
                Forex::BankOfCanada(_) => canada::build_response_body(yesterday_timestamp, rates),
                Forex::CentralBankOfUzbekistan(_) => {
                    uzbekistan::build_response_body(yesterday_timestamp, rates)
                }
                Forex::ReserveBankOfAustralia(_) => {
                    australia::build_response_body(yesterday_timestamp, rates)
                }
                Forex::CentralBankOfNepal(_) => {
                    nepal::build_response_body(yesterday_timestamp, rates)
                }
                Forex::CentralBankOfGeorgia(_) => {
                    georgia::build_response_body(yesterday_timestamp, rates)
                }
                Forex::BankOfItaly(_) => italy::build_response_body(yesterday_timestamp, rates),
                Forex::SwissFederalOfficeForCustoms(_) => {
                    switzerland::build_response_body(yesterday_timestamp, rates)
                }
            })
            .unwrap_or_default();
        ExchangeResponse::builder()
            .name(forex.to_string())
            .url(url)
            .body(body)
            .build()
    })
}
