# Proposal to upgrade the exchange rate canister

Repository: `https://github.com/dfinity/exchange-rate-canister.git`

Git hash: `bd9a4892e31115e4dcd5d1f7fd72eefefa2fd91d`

New compressed Wasm hash: `42f2a0a1c4fa9a174ca0732feb1a3a9d14b79ceaee693b66c7b4657672254c62`

Upgrade args hash: `0fee102bd16b053022b69f2c65fd5e2f41d150ce9c214ac8731cfaf496ebda4e`

Target canister: `uf6dk-hyaaa-aaaaq-qaaaq-cai`

Previous exchange rate proposal: https://dashboard.internetcomputer.org/proposal/142566

---

## Motivation
- Add USDC as a privileged asset
- Validate composed rate on the cache-only crypto/fiat path
- Don't seed a stablecoin metrics gauge for an unqueried exchange


## Release Notes

```
git log --format='%C(auto) %h %s' a49a90599c4e7454df67a36b663d048883dcd0e3..bd9a4892e31115e4dcd5d1f7fd72eefefa2fd91d --
bd9a489 feat: DEFI-2916: add USDC as a privileged asset (#352)
b1a82c8 fix: DEFI-2896: validate composed rate on the cache-only crypto/fiat path (#345)
20495f6 fix: DEFI-2980: don't seed a stablecoin gauge for an unqueried exchange (#350)
7803943 chore: Proposal to upgrade the XRC to release 2026.06.26 (#349)
 ```

## Upgrade args

```
git fetch
git checkout bd9a4892e31115e4dcd5d1f7fd72eefefa2fd91d
didc encode '()' | xxd -r -p | sha256sum
```

## Wasm Verification

Verify that the hash of the gzipped WASM matches the proposed hash.

```
git fetch
git checkout bd9a4892e31115e4dcd5d1f7fd72eefefa2fd91d
IP_SUPPORT="ipv4" "./scripts/docker-build"
sha256sum ./xrc.wasm.gz
```