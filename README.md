<div align="center">
<a href="https://www.webb.tools/">

  ![Webb Logo](./assets/webb_banner_light.png#gh-light-mode-only)

  ![Webb Logo](./assets/webb_banner_dark.png#gh-dark-mode-only)
  </a>
  </div>

# Webb Relayer 
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/webb-tools/relayer?style=flat-square)](https://github.com/webb-tools/relayer/releases/latest) [![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/webb-tools/relayer/check.yml?branch=main&style=flat-square)](https://github.com/webb-tools/relayer/actions) [![License Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=flat-square)](https://opensource.org/licenses/Apache-2.0) [![Twitter](https://img.shields.io/twitter/follow/webbprotocol.svg?style=flat-square&label=Twitter&color=1DA1F2)](https://twitter.com/webbprotocol) [![Telegram](https://img.shields.io/badge/Telegram-gray?logo=telegram)](https://t.me/webbprotocol) [![Discord](https://img.shields.io/discord/833784453251596298.svg?style=flat-square&label=Discord&logo=discord)](https://discord.gg/cv8EfJu3Tn)


<!-- TABLE OF CONTENTS -->
<h2 id="table-of-contents" style=border:0!important> 📖 Table of Contents</h2>

<details open="open">
  <summary>Table of Contents</summary>
  <ul>
    <li><a href="#start"> Getting Started</a></li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#config"> Configuration</a></li>
    <li><a href="#api">API</a></li>
    <li><a href="#test">Testing</a></li>
  </ul>  
</details>

<h2 id="start"> Getting Started  🎉 </h2>

In the Webb Protocol, the relayer plays a variety of roles. This repo contains code for an Anchor System oracle, transaction and data relayer, and protocol governance participant. The aim is that these can all be run exclusive to one another to ensure maximum flexibility of external participants to the Webb Protocol.

The relayer system is composed of three main components. Each of these components should be thought of as entirely separate because they could be handled by different entities entirely.

1. Private transaction relaying (of user bridge transactions like Tornado Cash’s relayer)
2. Data querying (for zero-knowledge proof generation)
3. Event listening, proposing, and signature relaying (of DKG proposals where the relayer acts like an oracle)

#### Transaction relaying role

Relayers who fulfill the role of a transaction relayer are responsible with exposing an API for clients who wish to relay their zero-knowledge transactions through and with submitting them. Relayers of this role must possess enough balance on the blockchains in which they will relay these transactions, since, after all, they must possess the native balance to pay the fees for these transactions. Relayers can be configured for any number of chains and protocols from mixers to variable anchors and run for individual chains or all of them that exist for a given bridged set of anchors.

#### Data querying role

Relayers who fulfill this role do so in conjunction with the transaction relaying role although it is not required to possess both. Namely, this role is concerned with listening to the events occurring within an Anchor Protocol instance and storing the data for clients who wish to quickly access it through traditional HTTP methods. This role is actively maintained and sees regular updates to how we hope to store and serve data in the future.

#### Oracle role

Relayers who fulfill the role of an oracle listen to the Anchor Protocol instances on the various chains the anchors exist on. When they hear of insertions into the anchors' merkle trees they handle them accordingly (as is implemented in the event watchers). Those playing this role then relay the anchor update information to other connected Anchors, the DKG governance system, and any other integration that gets implemented in this repo. Oracle relayers help keep the state of an Anchor Protocol instance up to date by ensuring that all anchors within an instance know about the latest state of their neighboring anchors.

For additional information, please refer to the [Webb Relayer Rust Docs](https://webb-tools.github.io/relayer/) 📝. Have feedback on how to improve the relayer network? Or have a specific question to ask? Checkout the [Relayer Feedback Discussion](https://github.com/webb-tools/feedback/discussions/categories/webb-relayer-feedback) 💬.

### Prerequisites

This repo uses Rust so it is required to have a Rust developer environment set up. First install and configure rustup:

```bash
# Install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Configure
source ~/.cargo/env
```

Configure the Rust toolchain to default to the latest stable version:

```bash
rustup default stable
rustup update
```

Great! Now your Rust environment is ready! 🚀🚀

Lastly, install 

  - [DVC](https://dvc.org/) is used for fetching large ZK files and managing them alongside git
  - [substrate.io](https://docs.substrate.io/main-docs/install/) may require additional dependencies

🚀🚀 Your environment is complete! 🚀🚀

### Installation 💻

#### Unix (Linux, macOS, WSL2, ..)

```
git clone https://github.com/webb-tools/relayer.git

cargo build --release --features cli
```

<h2 id="usage"> Usage </h2>

### Quick Start ⚡

#### Local EVM Setup

Eager to try out the Webb Relayer and see it in action? Run a relayer with our preset EVM Local Network configuration to get up and running immediately. You can follow this [guide](https://github.com/webb-tools/webb-dapp/tree/develop/apps/bridge-dapp#run-local-webb-relayer-and-local-network-alongside-hubble-bridge) to use the relayer for the EVM bridge! You will have to configure an `.env` file in the root directory as well. See below configuration section for more details.

```bash
# Update your local env file
cp ./config/development/evm-localnet/.env.example .env
cargo run --bin webb-relayer --features cli -- -c ./config/development/evm-localnet -vvv
```

> Hot Tip 🌶️: To increase the logger verbosity add additional `-vvvv` during start up command. You will now see `TRACE` logs. Happy debugging!

#### Local Substrate Mixer

To use the relayer for our Substrate mixer, you will first need to start a local substrate node that integrates with our pallets [webb-standalone-node](https://github.com/webb-tools/protocol-substrate/). Once the Substrate node is started locally you can proceed to start the relayer.

```
cargo run --bin webb-relayer --features cli -- -c ./config/development/local-substrate -vvv
```

### Run 🏃

Webb Relayer is easy to run and with flexible config 👌. The first step is to create a config file.

Example:

- Create an `.env` file with the following values for the networks you wish to support.

```
WEBB_EVM_<network>_ENABLED=true
WEBB_EVM_<network>_PRIVATE_KEY=<0X_PREFIXED_PRIVATE_KEY>

WEBB_EVM_<network>_BENEFICIARY=<0X_PREFIXED_ADDRESS>
```

> Checkout [config](./config) for useful default configurations for many networks. These config files can be changed to your preferences, and are enabled with the .env configuration listed above.

Then run:

```
webb-relayer -vv -c ./config
```

> Hot Tip 🌶️: you could also use the `json` format for the config files if you prefer that!

<h2 id="config"> Configuration </h2>

**Note:** You can also review the different chain configurations for EVM and Substrate.

- [`SubstrateConfig`](https://webb-tools.github.io/relayer/webb_relayer/config/struct.SubstrateConfig.html)
- [`EvmChainConfig`](https://webb-tools.github.io/relayer/webb_relayer/config/struct.EvmChainConfig.html)

#### Chain Configuration

| Field           | Description                                                                                                                        | Optionality            |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ---------------------- |
| `http-endpoint` | Http(s) Endpoint for quick Req/Res                                                                                                 | Required               |
| `ws-endpoint`   | Websocket Endpoint for long living connections                                                                                     | Required               |
| `name`          | The Chain/Node name                                                                                                                | Required               |
| `explorer`      | Block explorer, used for generating clickable links for transactions that happens on this chain.                                   | Optional               |
| `chain-id`      | Chain specific id.                                                                                                                 | Required               |
| `private-key`   | The Private Key of this account on this network. See [PrivateKey Docs for secure setup]()                                          | Required               |
| `beneficiary`   | The address of the account that will receive relayer fees.                                                                         | Optional               |
| `runtime`       | Indicates Substrate runtime to use                                                                                                 | Required for Substrate |
| `suri`          | Interprets a string in order to generate a key Pair. In the case that the pair can be expressed as a direct derivation from a seed | Required for Substrate |
| `pallets`       | Supported pallets for a particular Substrate node                                                                                  | Optional               |

#### Contract Configuration

| Field                      | Description                                                                                | Optionality |
| -------------------------- | ------------------------------------------------------------------------------------------ | ----------- |
| `contract`                 | Chain contract. Must be either: </br> - VAnchor </br> - SignatureBridge </br>              | Required    |
| `address`                  | The address of this contract on this chain.                                                | Required    |
| `deployed-at`              | The block number where this contract got deployed at.                                      | Required    |
| `events-watcher`           | Control the events watcher for this contract.                                              | Optional    |
| `withdraw-config`          | Config the fees and gas limits of your private transaction relayer.                        | Optional    |
| `proposal-signing-backend` | a value of `ProposalSigingBackend` (for example `{ type = "DKGNode", node = "dkg-node" }`) | Optional    |

### Docker 🐳

To use Docker to run the relayer, you will need to specify a config file and provide an `.env` file as described above. Then proceed to save it into the `config` directory.

To run docker image:

```sh
docker run --rm -v "<ABSOLUTE_PATH_TO_CONFIGS_DIRECTORY>:/config" --env-file .env -p 9955:9955 ghcr.io/webb-tools/relayer:edge
```

> Note: this uses the latest and pre-released version deployed from `main` branch, change `edge` to the latest stable release version.

This will mount a configuration files at the `/config` directory inside the container so it would allow it to read the configuration you added.

#### Metrics Information
 The Metric information is being handled by prometheus and the Relayer supports the following metrics:
 1. The number of times the `BridgeWatcher` enter backoff
 2. The number of times the `handle_proposal` executes
 3. The number of times the Transaction Queue enters backoff
 4. The number of times a Proposal attempted to be queued
 5. Total `Fees` Earned by the relayer
 6. Total `transaction` made
 7. Total `gas` spent
 8. Number of `proposals` proposed
 9. Amount of `data` stored

<h2 id="api"> API  📡</h2>

The relayer has 3 endpoints available to query from. They are outlined below for your convenience.

**Retrieving nodes IP address:**

```
/api/v1/ip
```

<details>
  <summary>Expected Response</summary>
  
  ```json
{
    "ip": "127.0.0.1"
}
  ```
</details>

**Retrieve relayer configuration**

```
/api/v1/info
```

<details>
  <summary>Expected Response</summary>
  
  ```json
  {
    "evm": {
        "rinkeby": {
            "enabled": true,
            "chainId": 4,
            "beneficiary": "0x58fcd47ece3ed24ace88fee06efd90dcb38f541f",
            "contracts": [{
                "contract": "Anchor",
                "address": "0x626fec5ffa7bf1ee8ced7dabde545630473e3abb",
                "deployedAt": 8896800,
                "eventsWatcher": {
                    "enabled": true,
                    "pollingInterval": 15000
                },
                "size": 0.1,
                "proposalSigningBackend": { "type": "DKGNode", "node": "dkg-local" },
                "withdrawFeePercentage": 0.05
            }]
        }
    },
    "substrate": {},
    "experimental": {
        "smart-anchor-updates": false,
        "smart-anchor-updates-retries": 0
    }
}
  ```
</details>

**Retrieve historical leaves cache**

##### Parameters

- `target_system` (Could be `evm` or `substrate`).
- `chain_id`
- `contract_address`

##### For evm
```
/api/v1/leaves/evm/4/0x626fec5ffa7bf1ee8ced7dabde545630473e3abb
```

##### For substrate
> Note: Since substrate dosent have contract address we use `tree_id`
```
/api/v1/leaves/substrate/4/9
```

<details>
  <summary>Expected Response</summary>
  
  ```json
   {
    "leaves": ["0x2e5c62af48845c095bfa9b90b8ec9f6b7bd98fb3ac2dd3039050a64b919951dd", "0x0f89f0ef52120b8db99f5bdbbdd4019b5ea4bcfef14b0c19d261268da8afdc24", "0x3007c62f678a503e568534487bc5b0bc651f37bbe1f34668b4c8a360f15ba3c3"],
    "lastQueriedBlock": "0x9f30a8"
}
  ```
</details>


**Retrieve Metrics information**

```
/api/v1/metrics
```

<details>
  <summary>Expected Response</summary>

  ```json
  {
  "metrics": "# HELP bridge_watcher_back_off_metric specifies how many times the bridge watcher backed off\n# TYPE bridge_watcher_back_off_metric counter\nbridge_watcher_back_off_metric 0\n# HELP gas_spent_metric The total number of gas spent\n# TYPE gas_spent_metric counter\ngas_spent_metric 0\n# HELP handle_proposal_execution_metric How many times did the function handle_proposal get executed\n# TYPE handle_proposal_execution_metric counter\nhandle_proposal_execution_metric 0\n# HELP proposal_queue_attempt_metric How many times a proposal is attempted to be queued\n# TYPE proposal_queue_attempt_metric counter\nproposal_queue_attempt_metric 0\n# HELP total_active_relayer_metric The total number of active relayers\n# TYPE total_active_relayer_metric counter\ntotal_active_relayer_metric 0\n# HELP total_fee_earned_metric The total number of fees earned\n# TYPE total_fee_earned_metric counter\ntotal_fee_earned_metric 0\n# HELP total_number_of_data_stored_metric The Total number of data stored\n# TYPE total_number_of_data_stored_metric counter\ntotal_number_of_data_stored_metric 1572864\n# HELP total_number_of_proposals_metric The total number of proposals proposed\n# TYPE total_number_of_proposals_metric counter\ntotal_number_of_proposals_metric 0\n# HELP total_transaction_made_metric The total number of transaction made\n# TYPE total_transaction_made_metric counter\ntotal_transaction_made_metric 0\n# HELP transaction_queue_back_off_metric How many times the transaction queue backed off\n# TYPE transaction_queue_back_off_metric counter\ntransaction_queue_back_off_metric 0\n"
}
  ```
</details>


**Retrieve fee information**

```
/api/v1/fee_info
```

##### Parameters

- `chain_id`
- `contract_address`

<details>
  <summary>Expected Response</summary>

  ```json
  {
      "estimatedFee": "0x476b26e0f",
      "gasPrice": "0x11",
      "refundExchangeRate": "0x28f",
      "maxRefund": "0xf3e59",
      "timestamp": "2023-01-19T06:29:49.556114073Z"
  }
  ```
</details>

<h2 id="test"> Testing 🧪 </h2>

The following instructions outlines how to run the relayer base test suite and E2E test suite.

### To run base tests

```
cargo test
```

### To run E2E tests

First you will need [`protocol-substrate`](https://github.com/webb-tools/protocol-substrate) node, compiled locally (in release mode) and both the `protocol-substrate` and `relayer` project must be next to each other. The relayer must be compiled using `--features integration-tests,cli`.

Here is the basic setup you will need:

1. Clone the Relayer repo `git clone https://github.com/webb-tools/relayer.git` 
2. Clone Protocol Substrate node `https://github.com/webb-tools/protocol-substrate.git`
3. Then fetch the submodules for the node `cd protocol-substrate && git submodule update --init`
4. While you are there, build the standalone node `cargo build --release -p webb-standalone-node`
5. And then go back to the relayer `cd ../relayer`
6. Run `cd tests && dvc pull`
7. Run `yarn install` (in `tests` dir)
8. `yarn test`

### Tips for E2E tests

1. If you want to run a specific test run `yarn test -fgrep <UNIQUE_PART_OF_TEST_NAME>`.
2. If you want to make the tests fail fast (fail on first error) run `yarn test --bail`.
3. by default, tests runs in parallel, to disable that run `yarn test --parallel=false`.
4. failing tests will keep retry before giving up, up to 5 times. To disable that use `yarn test --retries=0`.
5. You can combine all the tips above together, for more options see [here](https://mochajs.org/#command-line-usage)

For the Substrate Mixer test, you can connect to your local chain manually by:

1. Specifying the Alice node ports such as:

```ts
const aliceManualPorts = { ws: 9944, http: 9933, p2p: 30333 };
```

2. Specifying the Bob node ports such as:

```ts
const bobManualPorts = { ws: 9945, http: 9934, p2p: 30334 };
```

3. Make the `ports` property value be the `aliceManualPorts` and `bobManualPorts` respectively in the `LocalNodeOpts` config which is the parameter in `LocalProtocolSubstrate.start()` method.
4. Specifying and setting `isManual` flag to `true` in the `LocalNodeOpts` config which is the parameter in `LocalProtocolSubstrate.start()` method.

## Contributing

Interested in contributing to the Webb Relayer Network? Thank you so much for your interest! We are always appreciative for contributions from the open-source community!

If you have a contribution in mind, please check out our [Contribution Guide](./.github/CONTRIBUTING.md) for information on how to do so. We are excited for your first contribution!

## License

Licensed under <a href="LICENSE">Apache 2.0 license</a>.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache 2.0 license, shall
be licensed as above, without any additional terms or conditions.

## Supported by

<br />
<p align="center">
 <img src="./assets/w3f.jpeg" width="30%" height="60%" >
</p>
