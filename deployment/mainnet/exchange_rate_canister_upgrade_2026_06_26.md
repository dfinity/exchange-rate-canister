# Proposal to upgrade the exchange rate canister

Repository: `https://github.com/dfinity/exchange-rate-canister.git`

Git hash: `a49a90599c4e7454df67a36b663d048883dcd0e3`

New compressed Wasm hash: `eee5729e8caf4725d64e3b709ad942b2cab28663031192430bc275b914927222`

Upgrade args hash: `0fee102bd16b053022b69f2c65fd5e2f41d150ce9c214ac8731cfaf496ebda4e`

Target canister: `uf6dk-hyaaa-aaaaq-qaaaq-cai`

Previous exchange rate proposal: https://dashboard.internetcomputer.org/proposal/142448

---

## Motivation
- Drop CryptoCom USDT-USDC stablecoin source
- Record empty exchange windows as `no_data` rather than `http_error`


## Release Notes

```
git log --format='%C(auto) %h %s' 8983921987e111cb76d7dd7ae23cbc3fa409b3a1..a49a90599c4e7454df67a36b663d048883dcd0e3 --
a49a905 fix: DEFI-2890: drop CryptoCom USDT-USDC stablecoin source (#341)
7c98ff1 feat: DEFI-2903: record empty exchange windows as no_data, not http_error (#347)
e9f97ba chore: Proposal to upgrade the XRC to release 2026.06.19 (#346)
 ```

## Upgrade args

```
git fetch
git checkout a49a90599c4e7454df67a36b663d048883dcd0e3
didc encode '()' | xxd -r -p | sha256sum
```

## Wasm Verification

Verify that the hash of the gzipped WASM matches the proposed hash.

```
git fetch
git checkout a49a90599c4e7454df67a36b663d048883dcd0e3
IP_SUPPORT="ipv4" "./scripts/docker-build"
sha256sum ./xrc.wasm.gz
```