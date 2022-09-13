use ic_cdk::export::candid::{CandidType, Deserialize};

#[derive(CandidType, Clone, Debug, Deserialize)]
pub enum AssetClass {
    /// The cryptocurrency asset class.
    Cryptocurrency,
    /// The fiat currency asset class.
    FiatCurrency,
}

#[derive(CandidType, Clone, Debug, Deserialize)]
pub struct Asset {
    /// The symbol/code of the asset.
    pub symbol: String,
    /// The asset class.
    pub class: AssetClass,
}

/// The type the user sends when requesting a rate.
#[derive(CandidType, Deserialize)]
pub struct GetExchangeRateRequest {
    /// The asset to be used as the resulting asset. For example, using
    /// ICP/USD, ICP would be the base asset.
    pub base_asset: Asset,
    /// The asset to be used as the starting asset. For example, using
    /// ICP/USD, USD would be the quote asset.
    pub quote_asset: Asset,
    /// An optional parameter used to find a rate at a specific time.
    pub timestamp: Option<u64>,
}

/// Metadata information to give background on how the rate was determined.
#[derive(CandidType, Clone, Debug, Deserialize)]
pub struct ExchangeRateMetadata {
    /// The number of exchanges queried to determine the results.
    pub number_of_queried_sources: u64,
    /// The number rates successfully received from the queried sources.
    pub number_of_received_rates: u64,
    /// The standard deviation of the received rates.
    pub standard_deviation_permyriad: u64,
}

/// When a rate is determined, this struct is used to present the information
/// to the user.
#[derive(CandidType, Clone, Debug, Deserialize)]
pub struct ExchangeRate {
    /// The base asset.
    pub base_asset: Asset,
    /// The quote asset.
    pub quote_asset: Asset,
    /// The timestamp associated with the returned rate.
    pub timestamp: u64,
    /// The median rate from the received rates in permyriad.
    pub rate_permyriad: u64,
    /// Metadata providing additional information about the exchange rate calculation.
    pub metadata: ExchangeRateMetadata,
}

// TODO: define more concrete error types instead of a generic when we have a
// better understanding of the types of errors we would like to return.
/// Returned to the user when something goes wrong retrieving the exchange rate.
#[derive(CandidType, Deserialize)]
pub struct ExchangeRateError {
    /// The identifier for the error that occurred.
    pub code: u32,
    /// A description of the error that occurred.
    pub description: String,
}

/// Short-hand for returning the result of a `get_exchange_rate` request.
pub type GetExchangeRateResult = Result<ExchangeRate, ExchangeRateError>;
