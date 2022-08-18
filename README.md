# Exchange Rate Canister

## Official build
The official build should ideally be reproducible, so that independent parties can validate that we really deploy what we claim to deploy.

We try to achieve some level of reproducibility using a Dockerized build environment. The following steps should build the official Wasm image

```bash
./scripts/docker-build
sha256sum xrc.wasm
```

The resulting xrc.wasm is ready for deployment as **CANISTER ID HERE**, which is the reserved principal for this service.

Our CI also performs these steps; you can compare the SHA256 with the output there, or download the artifact there.
