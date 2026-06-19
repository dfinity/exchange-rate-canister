# Proposal to upgrade the exchange rate canister

Repository: `https://github.com/dfinity/exchange-rate-canister.git`

Git hash: `8983921987e111cb76d7dd7ae23cbc3fa409b3a1`

New compressed Wasm hash: `eac08779115af8771eec2e44e479b90b6a42b47a06765849690f8da9c6c435f5`

Upgrade args hash: `0fee102bd16b053022b69f2c65fd5e2f41d150ce9c214ac8731cfaf496ebda4e`

Target canister: `uf6dk-hyaaa-aaaaq-qaaaq-cai`

Previous exchange rate proposal: https://dashboard.internetcomputer.org/proposal/142255

---

## Motivation
- Never cache or return an empty/zero post-filter crypto rate
- Only query crypto exchanges for rates for which a base/USDT trading pair is explicitly listed


## Release Notes

```
git log --format='%C(auto) %h %s' 99b3c5be32a99e79006032321d46236c9400e107..8983921987e111cb76d7dd7ae23cbc3fa409b3a1 --
8983921 fix: DEFI-2896: never cache or return an empty/zero post-filter crypto rate (#343)
e0277e1 feat: DEFI-2868: gate crypto queries by discovered listings (#337)
c3f9e2e fix: DEFI-2648: migrate monitor-canister off removed ic-cdk call APIs (#334)
8c5817a test: DEFI-2868: canbench benchmarks for listing parsers and regression gate (#331)
1dcb821 chore: Proposal to upgrade the XRC to release 2026.06.12 (#340)
 ```

## Upgrade args

```
git fetch
git checkout 8983921987e111cb76d7dd7ae23cbc3fa409b3a1
didc encode '()' | xxd -r -p | sha256sum
```

## Wasm Verification

Verify that the hash of the gzipped WASM matches the proposed hash.

```
git fetch
git checkout 8983921987e111cb76d7dd7ae23cbc3fa409b3a1
IP_SUPPORT="ipv4" "./scripts/docker-build"
sha256sum ./xrc.wasm.gz
```