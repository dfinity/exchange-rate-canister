# Proposal to upgrade the exchange rate canister

Repository: `https://github.com/dfinity/exchange-rate-canister.git`

Git hash: `0aba8304d870549a2afb71ed2db87e15c036b164`

New compressed Wasm hash: `cc65d2f5a4a00a3506538801af3b36ee183980156a91e58ce31c05ead298a7e6`

Upgrade args hash: `0fee102bd16b053022b69f2c65fd5e2f41d150ce9c214ac8731cfaf496ebda4e`

Target canister: `uf6dk-hyaaa-aaaaq-qaaaq-cai`

Previous exchange rate proposal: https://dashboard.internetcomputer.org/proposal/141774

---

## Motivation
Add metrics to be able to track and detect issues and outages related to exchange rate retrievals.


## Release Notes

```
git log --format='%C(auto) %h %s' 918d34cbbeccf384b595fb0171a931e2638020ab..0aba8304d870549a2afb71ed2db87e15c036b164 --
0aba830 test: DEFI-2810: E2e coverage for labeled metrics and doc cleanups (#310)
502f263 feat: DEFI-2810: Add per-exchange and per-stablecoin observability (#309)
50ae2ae feat: DEFI-2810: Add labeled metrics infrastructure and per-forex observability (#308)
 ```

## Upgrade args

```
git fetch
git checkout 0aba8304d870549a2afb71ed2db87e15c036b164
didc encode '()' | xxd -r -p | sha256sum
```

## Wasm Verification

Verify that the hash of the gzipped WASM matches the proposed hash.

```
git fetch
git checkout 0aba8304d870549a2afb71ed2db87e15c036b164
IP_SUPPORT="ipv4" "./scripts/docker-build"
sha256sum ./xrc.wasm.gz
```
