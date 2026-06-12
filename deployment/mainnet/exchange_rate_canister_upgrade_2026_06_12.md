# Proposal to upgrade the exchange rate canister

Repository: `https://github.com/dfinity/exchange-rate-canister.git`

Git hash: `99b3c5be32a99e79006032321d46236c9400e107`

New compressed Wasm hash: `712b424f1346e94373b5fbede822fa7d7a3a8580f13916513d77b3ca6ca71d3a`

Upgrade args hash: `0fee102bd16b053022b69f2c65fd5e2f41d150ce9c214ac8731cfaf496ebda4e`

Target canister: `uf6dk-hyaaa-aaaaq-qaaaq-cai`

Previous exchange rate proposal: https://dashboard.internetcomputer.org/proposal/142133

---

## Motivation
- Discover trading pairs dynamically across exchanges, in preparation for only querying rates from exchanges where the pair is traded.
- Query KuCoin over a closed-minute window to avoid errors for the still-forming minute.


## Release Notes

```
git log --format='%C(auto) %h %s' d34c21acb582afb872c93b14559e23ea9fb07c7a..99b3c5be32a99e79006032321d46236c9400e107 --
99b3c5b fix: release the XRC-call guard when entry encoding fails (#335)
b609418 feat: DEFI-2868: discover listings periodically - store, refresh, metrics (#332)
8145b19 feat: DEFI-2868: add per-exchange listing parsers for USDT pair discovery (#330)
75c3ec9 fix: DEFI-2866: query KuCoin over a closed-minute window (#326)
ff74e66 chore: Proposal to upgrade the XRC to release 2026.06.05 (#325)
```

## Upgrade args

```
git fetch
git checkout 99b3c5be32a99e79006032321d46236c9400e107
didc encode '()' | xxd -r -p | sha256sum
```

## Wasm Verification

Verify that the hash of the gzipped WASM matches the proposed hash.

```
git fetch
git checkout 99b3c5be32a99e79006032321d46236c9400e107
IP_SUPPORT="ipv4" "./scripts/docker-build"
sha256sum ./xrc.wasm.gz
```
