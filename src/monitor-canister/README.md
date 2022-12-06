# Monitor Canister

The monitor canister is used to collect rates over a period of time.

## Install

After deploying the exchange rate canister (`xrc`), the monitor canister can be deployed.
This is done by providing the `xrc`'s canister ID to the monitor canister's init arguments.

```bash
dfx deploy monitor-canister --argument '(record { xrc_canister_id = principal "<canister-id>" })'
```

Replace `<canister-id>` with the `xrc`'s canister ID.

## Recording entries into the canister

The monitor canister records entries into the canister using the canister's heartbeat.

## Retrieving entries from the canister

To retrieve records from the monitor canister, call the `get_entries` endpoint. The monitor
canister records entries in a log structure going from oldest to newest.

```bash
# start at index zero and retrieve 20 records
dfx canister call monitor-canister get_entries '(record { offset = 0; })'

# start at index 50 and retrieve 10 records
dfx canister call monitor-canister get_entries '(record { offset = 50; limit = opt 10; })'
```