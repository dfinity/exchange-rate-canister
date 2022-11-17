# Exchange Rate Canister

<div>
  <p>
    <a href="https://github.com/dfinity/exchange-rate-canister/blob/master/LICENSE"><img alt="Apache-2.0" src="https://img.shields.io/github/license/dfinity/exchange-rate-canister"/></a>
    <a href="https://forum.dfinity.org/"><img alt="Chat on the Forum" src="https://img.shields.io/badge/help-post%20on%20forum.dfinity.org-yellow"></a>
  </p>
</div>

## Overview
The exchange rate canister provides an oracle service for cryptocurrency and
fiat currency exchange rates.
It interacts with all data sources using the
[HTTPS outcalls](https://internetcomputer.org/https-outcalls/) feature.


## Usage

The exchange rate canister offers a single endpoint:

```
"get_exchange_rate": (GetExchangeRateRequest) -> (GetExchangeRateResult)
```
The request must specify the base and quote assets and, optionally, a timestamp.
The returned result contains either the exchange rate for the requested asset pair
along with some metadata or an error.
Details can be found in the [Candid file](src/xrc/xrc.did).

> **_NOTE:_** 5B cycles must be sent to the exchange rate canister with each request.
A certain amount may be refunded depending on the number of required HTTPs outcalls
to serve the request. The base fee is 200M cycles.

## Official build
The official build should ideally be reproducible, so that independent parties
can validate that the correct WebAssembly module was deployed.

A Dockerized build environment is used to build the WebAssembly module and
print its SHA-256 hash.

```bash
./scripts/docker-build
sha256sum xrc.wasm
```

The canister ID of the beta version is `gvu7d-7aaaa-aaaan-aaaba-cai`.

> **_NOTE:_** The beta release may be removed once the main release has been deployed.
The main release will have a different canister ID.

## Contribution mode
External contributions are accepted.
