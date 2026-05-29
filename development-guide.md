# Development Guide

This guide documents how to cut a release of the exchange rate canister (XRC)
and deploy it to the Internet Computer via an NNS upgrade proposal. The process
mirrors the one used by the sibling
[bitcoin-canister](https://github.com/dfinity/bitcoin-canister/blob/master/development-guide.md)
and
[dogecoin-canister](https://github.com/dfinity/dogecoin-canister/blob/master/development-guide.md).

## Release Overview

The repository builds a single release artifact, the exchange rate canister
(`xrc`). It uses **date-based** release tags of the form `YYYY.MM.DD` (e.g.
`2026.05.29`) and is **not** published to crates.io. (`xrc_mock` is a test
helper bundled into the same release for convenience; it is not deployed to
production.)

### Canister IDs

| Network            | Canister ID                   |
|--------------------|-------------------------------|
| Production (`ic`)  | `uf6dk-hyaaa-aaaaq-qaaaq-cai` |

The exchange rate canister is deployed in production by submitting proposals to
the Internet Computer's [Network Nervous System](https://internetcomputer.org/nns).

## Releasing the Canister

Releasing is a button-driven, two-step process followed by an NNS proposal.

### Step 1: Create a Release PR

1. Go to **Actions → Create Release PR**
2. Click **Run workflow**

This creates a **draft** PR that updates `CHANGELOG.md` using
[git-cliff](https://git-cliff.org/), grouping the commits since the previous
release tag.

3. Review and merge the PR.

### Step 2: Create the GitHub Release

1. Go to **Actions → Create GitHub Release**
2. Click **Run workflow**

This creates a **draft** GitHub release tagged `YYYY.MM.DD` containing:

- the `xrc.wasm.gz` and `xrc_mock.wasm.gz` artifacts,
- the candid file (`src/xrc/xrc.did`),
- the changelog scoped to this release,
- the SHA-256 checksum of `xrc.wasm.gz`, and
- a placeholder for the NNS proposal link.

3. Review the draft release.

### Step 3: Deploy via an NNS Proposal

Create and submit the upgrade proposal (see
[Creating the Upgrade Proposal](#creating-the-upgrade-proposal) below). Once the
proposal has been submitted, update the release notes with the proposal link
and mark the release as **Latest**.

## Reproducible WASM Build (for verification)

The release WASM is built reproducibly with Docker. Anyone can rebuild it from a
given commit and confirm the SHA-256 matches the hash in the release notes and
the proposal:

```shell
# Check out the release commit
git checkout <commit-sha>

# Build reproducibly with Docker
IP_SUPPORT="ipv4" ./scripts/docker-build

# Verify the checksum matches the release / proposal
sha256sum xrc.wasm.gz
```

> **Note:** Reproducible builds require Docker. There is no reproducibility
> guarantee on Mac M1s; preferably use Ubuntu or Intel machines.

## Creating the Upgrade Proposal

The proposal artifacts are generated with `proposal-cli`, which lives in the
[IC monorepo](https://github.com/dfinity/ic) under
`rs/cross-chain/proposal-cli`. Run the commands below **from a checkout of the
IC monorepo**.

**Prerequisites:**

- `bazel` (used to run `proposal-cli`),
- an HSM holding the proposer key, with its PIN in `~/.hsm-pin` (required when submitting the proposal, not when running `proposal-cli`).

### Step 1: Generate the proposal artifacts

```shell
bazel run //rs/cross-chain/proposal-cli:make_proposal -- upgrade exchange-rate \
  --from <previous-release-git-sha> \
  --to <new-release-git-sha> \
  --output-dir <output-dir> \
  --args '()' \
  ic-admin --use-hsm --key-id <key-id> --slot <slot> --pin "$(cat ~/.hsm-pin)" --proposer <proposed-neuron-id>
```

This writes the proposal summary (git hash, gzipped WASM hash, upgrade-args
hash, target canister, release notes and verification instructions) and the
`ic-admin` submission command into the output directory.

> Note: The `ic-admin` command parameters may vary depending on the proposer's individual setup.

### Step 2: Commit the summary for review

The proposal summary doubles as the in-repo review gate:

1. Open the generated summary and add a hand-written `## Motivation` section
   explaining *why* this upgrade is being proposed.
2. Commit it to this repository as
   `deployment/mainnet/exchange_rate_canister_upgrade_YYYY_MM_DD.md`.
3. Open a PR.

**Reviewing and merging that PR is the review step for the proposal** — it lets
reviewers check the git hash, WASM hash, upgrade args and release notes before
anything is submitted to the NNS.

### Step 3: Submit to the NNS

Once the summary PR is approved, submit the proposal by running the `ic-admin`
command emitted by `proposal-cli` in Step 1. Note the proposal ID shown as part
of the output after successful submission of the proposal.

### Step 4: Create the forum post

```shell
bazel run //rs/cross-chain/proposal-cli:make_proposal -- create-forum-post \
  --api-key "<secret>" --api-user "<api-user>" <proposal-id>
```

### Step 5: Finalize the release

Update the GitHub release's `## Proposals` section with the proposal link and
mark the release as **Latest**.
