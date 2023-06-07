use xrc::{Forex, FOREX_SOURCES};

use crate::container::ExchangeResponse;

mod bank_of_canada;

pub fn build_responses(timestamp: u64) -> impl Iterator<Item = ExchangeResponse> + 'static {
    FOREX_SOURCES
        .iter()
        .filter(|forex| matches!(forex, Forex::BankOfCanada(_)))
        .map(move |forex| {
            let url = forex.get_url(timestamp);
            let body = match forex {
                Forex::MonetaryAuthorityOfSingapore(_) => todo!(),
                Forex::CentralBankOfMyanmar(_) => todo!(),
                Forex::CentralBankOfBosniaHerzegovina(_) => todo!(),
                Forex::EuropeanCentralBank(_) => todo!(),
                Forex::BankOfCanada(_) => bank_of_canada::build_response_body(timestamp),
                Forex::CentralBankOfUzbekistan(_) => todo!(),
                Forex::ReserveBankOfAustralia(_) => todo!(),
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
