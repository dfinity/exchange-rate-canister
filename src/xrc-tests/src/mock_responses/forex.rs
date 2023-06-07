use xrc::{Forex, FOREX_SOURCES};

use crate::container::ExchangeResponse;

mod austrailia;
mod canada;

pub fn build_responses(now_timestamp: u64) -> impl Iterator<Item = ExchangeResponse> + 'static {
    // Forex sources go back one day.
    let yesterday_timestamp =
        (now_timestamp.saturating_sub(86_400) / 86_400).saturating_mul(86_400);
    FOREX_SOURCES
        .iter()
        .filter(|forex| {
            matches!(
                forex,
                Forex::BankOfCanada(_) | Forex::ReserveBankOfAustralia(_)
            )
        })
        .map(move |forex| {
            let url = forex.get_url(yesterday_timestamp);
            let body = match forex {
                Forex::MonetaryAuthorityOfSingapore(_) => todo!(),
                Forex::CentralBankOfMyanmar(_) => todo!(),
                Forex::CentralBankOfBosniaHerzegovina(_) => todo!(),
                Forex::EuropeanCentralBank(_) => todo!(),
                Forex::BankOfCanada(_) => canada::build_response_body(yesterday_timestamp),
                Forex::CentralBankOfUzbekistan(_) => todo!(),
                Forex::ReserveBankOfAustralia(_) => {
                    austrailia::build_response_body(yesterday_timestamp)
                }
                Forex::CentralBankOfNepal(_) => todo!(),
                Forex::CentralBankOfGeorgia(_) => todo!(),
                Forex::BankOfItaly(_) => todo!(),
                Forex::SwissFederalOfficeForCustoms(_) => todo!(),
            };
            ExchangeResponse::builder()
                .name(forex.to_string())
                .url(url)
                .body(body)
                .build()
        })
}
