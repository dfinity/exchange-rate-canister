# Proposal to upgrade the exchange rate canister

Repository: `https://github.com/dfinity/exchange-rate-canister.git`

Git hash: `d34c21acb582afb872c93b14559e23ea9fb07c7a`

New compressed Wasm hash: `9a3595ec05751de1402d4b4b37f1f39500efa89a1190553663d077ac1a9a6ef6`

Upgrade args hash: `0fee102bd16b053022b69f2c65fd5e2f41d150ce9c214ac8731cfaf496ebda4e`

Target canister: `uf6dk-hyaaa-aaaaq-qaaaq-cai`

Previous exchange rate proposal: https://dashboard.internetcomputer.org/proposal/141980

---

## Motivation
-Send accepted User-Agent header in requests to Reserve Bank of Australia
-Fix inverted Reserve Bank of Australia rates
-Bump the maximum response size for requests to Bank of Canada
-Drop USDS/USDT pair as stablecoin source from KuCoin and Poloniex due to thin trading volume
-Order stablecoin bases so that the more stable USDC is chosen in case of a tie
-Query Coinbase over a closed-minute window and request last 6 minutes to avoid error in case of thin trading


## Release Notes

```
git log --format='%C(auto) %h %s' 0aba8304d870549a2afb71ed2db87e15c036b164..d34c21acb582afb872c93b14559e23ea9fb07c7a --
d34c21a fix: DEFI-2860: send a curl-style User-Agent for ReserveBankOfAustralia outcalls (#324)
f1055dc fix: DEFI-2855: invert RBA forex rates to USD-per-unit (#321)
578169d fix: DEFI-2859: bump BankOfCanada HTTP outcall max_response_bytes to 30 KiB (#323)
88332a5 fix: DEFI-2845: drop dead USDS-USDT pair from KuCoin stablecoin sources (#316)
9870e85 fix: DEFI-2845: drop dead, off-peg USDS-USDT pair from Poloniex stablecoin sources (#317)
8c38ca0 fix: DEFI-2845: order STABLECOIN_BASES most-trusted-first ([USDC, USDS]) (#320)
4fdc040 test: DEFI-2845: pin actual 2-symbol stablecoin selection behaviour (#319)
932f91c fix: DEFI-2810: query Coinbase over a closed-minute window (#314)
82d00d0 fix: DEFI-2810: Tune metrics help message character (#315)
1ca1567 chore: Proposal to upgrade the XRC to release 2026.05.29 (#311)
 ```

## Upgrade args

```
git fetch
git checkout d34c21acb582afb872c93b14559e23ea9fb07c7a
didc encode '()' | xxd -r -p | sha256sum
```

## Wasm Verification

Verify that the hash of the gzipped WASM matches the proposed hash.

```
git fetch
git checkout d34c21acb582afb872c93b14559e23ea9fb07c7a
IP_SUPPORT="ipv4" "./scripts/docker-build"
sha256sum ./xrc.wasm.gz
```
