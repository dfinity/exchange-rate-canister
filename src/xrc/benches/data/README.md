# Listing-endpoint captures

These JSON files are full-size, real responses from each exchange's public
spot-listing endpoint — the same URL the canister's listing refresh fetches
(`IsExchange::listing_url()` in `../../src/exchanges.rs`). They are the inputs
to the `canbench` listing-parser benchmarks in `../../src/benchmarks.rs`, so
they exercise `extract_listed_usdt_bases` on the same byte layout and size the
canister sees in production.

| File | Exchange | Listing endpoint |
|------|----------|------------------|
| `coinbase.json`  | Coinbase  | `https://api.exchange.coinbase.com/products` |
| `kucoin.json`    | KuCoin    | `https://api.kucoin.com/api/v1/symbols` |
| `okx.json`       | OKX       | `https://www.okx.com/api/v5/public/instruments?instType=SPOT` |
| `gateio.json`    | GateIo    | `https://api.gateio.ws/api/v4/spot/currency_pairs` |
| `mexc.json`      | MEXC      | `https://api.mexc.com/api/v3/defaultSymbols` |
| `poloniex.json`  | Poloniex  | `https://api.poloniex.com/markets` |
| `cryptocom.json` | Crypto.com| `https://api.crypto.com/exchange/v1/public/get-instruments` |
| `bitget.json`    | Bitget    | `https://api.bitget.com/api/v2/spot/public/symbols` |
| `digifinex.json` | Digifinex | `https://openapi.digifinex.com/v3/spot/symbols` |

## Regenerating

Fetch each endpoint and save the body **exactly as served** — do not pretty-print,
minify, or otherwise reformat it. Some exchanges (e.g. Crypto.com, Poloniex)
serve pretty-printed JSON; reformatting would change the byte layout and make the
benchmark under- or over-estimate the real parse cost.

```bash
cd src/xrc/benches/data

curl -sS "https://api.exchange.coinbase.com/products"                       -o coinbase.json
curl -sS "https://api.kucoin.com/api/v1/symbols"                            -o kucoin.json
curl -sS "https://www.okx.com/api/v5/public/instruments?instType=SPOT"      -o okx.json
curl -sS "https://api.gateio.ws/api/v4/spot/currency_pairs"                 -o gateio.json
curl -sS "https://api.mexc.com/api/v3/defaultSymbols"                       -o mexc.json
curl -sS "https://api.poloniex.com/markets"                                 -o poloniex.json
curl -sS "https://api.crypto.com/exchange/v1/public/get-instruments"        -o cryptocom.json
curl -sS "https://api.bitget.com/api/v2/spot/public/symbols"                -o bitget.json
curl -sS "https://openapi.digifinex.com/v3/spot/symbols"                    -o digifinex.json
```

After updating a capture, re-run the benchmarks and commit the refreshed
`../../canbench_results.yml` (see `../../src/benchmarks.rs` and the repo's
benchmark CI job).
