# Testing

## Dependencies

- Docker
- `docker-compose`

## How it works

The system testing harness uses `cargo`'s built-in testing framework. Each test
upon execution will create a Docker container that contains the following:

- `dfx`: used to install the canister, run a replica instance, and issue calls to the canister
- `nginx`: used to serve up mock exchanges
- `supervisor`: used to run an instance of `dfx replica` and `nginx` inside of the container

This is handled by calling `xrc_tests::container::run_scenario` with a
pre-configured instance of `Container`. The `Container` contains the responses
and the necessary mapping to the Docker container running in the background.
The `Container` instance also provides a method to calls to the canister
running inside of the container.

```rust
run_scenario(container, |container: &Container| {
    let output = container
        .call_canister::<_, Vec<u64>>("get_exchange_rates", request)
        .expect("Failed to call canister for rates");

    // Check if the rates found are in the order defined by the `exchanges!` macro call in exchanges.rs:56.
    assert_eq!(output, vec![419600, 482500, 3448330, 420300]);

    Ok(())
})
```

## Running the tests

To run the system tests manually, execute the following command:

```bash
./scripts/e2e-tests
```

This command does the following:

. Builds a reproducible build of the `xrc.wasm.gz`
. Moves the canister to the shared Docker volume
. Runs the system tests under the `xrc-tests` crate

The command tells `cargo` not to capture output. This allows debugging to be
easier especially if there is a failure in managing the Docker containers that
are created for each test.

## Adding an exchange

When adding a new exchange to the `xrc` canister, be sure to add a response
configuration as the test will make a call out to the actual exchange instead
of a mock.

## Using a custom WASM

Are tests failing? Want to add some debugging code? Make the necessary code
changes and run the following commands:

```bash
# Build the wasm without needing a canister
dfx build --check
# Build the e2e base image
docker compose -f src/xrc-tests/docker/docker-compose.yml build base
# Copy the built wasm to the target directory
mkdir -p src/xrc-tests/gen/canister
cp .dfx/local/canisters/xrc/xrc.wasm.gz src/xrc-tests/gen/canister
# Run the system tests
cargo test --tests --package xrc-tests -- --exact --nocapture
```
