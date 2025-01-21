## Exchange Rate Canister API

The canister ID of the cycles ledger is [`uf6dk-hyaaa-aaaaq-qaaaq-cai.`](https://dashboard.internetcomputer.org/canister/uf6dk-hyaaa-aaaaq-qaaaq-cai.).

The exchange rate canister exposes the following endpoint.

### `get_exchange_rate`
```
type AssetClass = variant { Cryptocurrency; FiatCurrency; };

type Asset = record {
    symbol: text;
    class: AssetClass;
};

// The parameters for the `get_exchange_rate` API call.
type GetExchangeRateRequest = record {
    base_asset: Asset;
    quote_asset: Asset;
    // An optional timestamp to get the rate for a specific time period.
    timestamp: opt nat64;
};

type ExchangeRateMetadata = record {
    decimals: nat32;
    base_asset_num_received_rates: nat64;
    base_asset_num_queried_sources: nat64;
    quote_asset_num_received_rates: nat64;
    quote_asset_num_queried_sources: nat64;
    standard_deviation: nat64;
    forex_timestamp: opt nat64;
};

type ExchangeRate = record {
    base_asset: Asset;
    quote_asset: Asset;
    timestamp: nat64;
    rate: nat64;
    metadata: ExchangeRateMetadata;
};

type ExchangeRateError = variant {
    // Returned when the canister receives a call from the anonymous principal.
    AnonymousPrincipalNotAllowed: null;
    /// Returned when the canister is in process of retrieving a rate from an exchange.
    Pending: null;
    // Returned when the base asset rates are not found from the exchanges HTTP outcalls.
    CryptoBaseAssetNotFound: null;
    // Returned when the quote asset rates are not found from the exchanges HTTP outcalls.
    CryptoQuoteAssetNotFound: null;
    // Returned when the stablecoin rates are not found from the exchanges HTTP outcalls needed for computing a crypto/fiat pair.
    StablecoinRateNotFound: null;
    // Returned when there are not enough stablecoin rates to determine the forex/USDT rate.
    StablecoinRateTooFewRates: null;
    // Returned when the stablecoin rate is zero.
    StablecoinRateZeroRate: null;
    // Returned when a rate for the provided forex asset could not be found at the provided timestamp.
    ForexInvalidTimestamp: null;
    // Returned when the forex base asset is found.
    ForexBaseAssetNotFound: null;
    // Returned when the forex quote asset is found.
    ForexQuoteAssetNotFound: null;
    // Returned when neither forex asset is found.
    ForexAssetsNotFound: null;
    // Returned when the caller is not the CMC and there are too many active requests.
    RateLimited: null;
    // Returned when the caller does not send enough cycles to make a request.
    NotEnoughCycles: null;
    // Returned when the canister fails to accept enough cycles.
    FailedToAcceptCycles: null;
    /// Returned if too many collected rates deviate substantially.
    InconsistentRatesReceived: null;
    // Until candid bug is fixed, new errors after launch will be placed here.
    Other: record {
        code: nat32;
        // A description of the error that occurred.
        description: text;
    }
};

type GetExchangeRateResult = variant {
    Ok: ExchangeRate;
    Err: ExchangeRateError;
};

get_exchange_rate: (GetExchangeRateRequest) -> (GetExchangeRateResult);
```

The endpoint takes a request for an exchange rate and returns a result. The request must specify a base asset and a quote asset. It can optionally specify a UNIX timestamp, in seconds, as well. If no timestamp is provided, the timestamp at the start of the current minute is used.

1B cycles must be attached to the call, otherwise it is rejected and a `NotEnoughCycles` error is returned. Depending on the number of HTTPS outcalls that are required to determine the requested rate, a certain amount of cycles may be refunded. The base fee is 200M cycles.

If the call is successful, the result will contain the requested exchange rate plus the timestamp, in seconds, for which the rate was determined and the base and quote assets.
Additionally, the result contains the following metadata:

* `decimals`: The rate is scaled by a factor of `10^decimals`.
* `base_asset_num_received_rates`: The number of rates received for the base asset.
* `base_asset_num_queried_sources`: The number of queried sources for the base asset.
* `quote_asset_num_received_rates`: The number of rates received for the quote asset.
* `quote_asset_num_queried_sources`: The number of queried sources for the quote asset.
* `standard_deviation`: The standard deviation of the received rates.
* `forex_timestamp`: If any forex rates are used to handle the request, this is the timestamp of the forex rates, which is always the timestamp at the beginning of a day.

If the call fails, the returned `ExchangeRateError` provides the reason. The different variants are shown above.
