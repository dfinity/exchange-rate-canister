use ic_cdk::export::candid::{CandidType, Deserialize};

#[derive(CandidType, Deserialize)]
pub struct GetExchangeRateRequest {
    pub timestamp: Option<u64>,
    pub quote_asset: String,
    pub base_asset: String,
}

#[derive(CandidType, Deserialize)]
pub struct ExchangeRateInformationMetadata {
    pub number_of_queried_sources: u64,
    pub spread: u64,
    pub number_of_received_rates: u64,
}

#[derive(CandidType, Deserialize)]
pub struct ExchangeRateInformation {
    pub rate_permyriad: u64,
    pub metadata: ExchangeRateInformationMetadata,
}

#[derive(CandidType, Deserialize)]
pub struct ExchangeRateError {
    pub code: u32,
    pub description: String,
}

pub type GetExchangeRateResult = Result<ExchangeRateInformation, ExchangeRateError>;
