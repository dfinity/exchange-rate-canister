use ic_cdk::export::candid::{CandidType, Deserialize};

/// The enum defining the different asset classes.
#[derive(CandidType, Clone, Debug, Deserialize, PartialEq)]
pub enum AssetClass {
    /// The cryptocurrency asset class.
    Cryptocurrency,
    /// The fiat currency asset class.
    FiatCurrency,
}

/// Exchange rates are derived for pairs of assets captured in this struct.
#[derive(CandidType, Clone, Debug, Deserialize, PartialEq)]
pub struct Asset {
    /// The symbol/code of the asset.
    pub symbol: String,
    /// The asset class.
    pub class: AssetClass,
}

/// The type the user sends when requesting a rate.
#[derive(CandidType, Clone, Deserialize)]
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
#[derive(CandidType, Clone, Debug, Deserialize, PartialEq)]
pub struct ExchangeRateMetadata {
    /// The number of queried exchanges.
    pub num_queried_sources: usize,
    /// The number of rates successfully received from the queried sources.
    pub num_received_rates: usize,
    /// The standard deviation of the received rates.
    pub standard_deviation_permyriad: u64,
}

/// When a rate is determined, this struct is used to present the information
/// to the user.
#[derive(CandidType, Clone, Debug, Deserialize, PartialEq)]
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
#[derive(CandidType, Debug, Deserialize)]
pub struct ExchangeRateError {
    /// The identifier for the error that occurred.
    pub code: u32,
    /// A description of the error that occurred.
    pub description: String,
}

/// Short-hand for returning the result of a `get_exchange_rate` request.
pub type GetExchangeRateResult = Result<ExchangeRate, ExchangeRateError>;
