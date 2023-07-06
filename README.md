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

#### Local Substrate Setup

To use the relayer for our Substrate based chains, you will first need to start a local substrate node that integrates with our pallets [webb-standalone-node](https://github.com/webb-tools/protocol-substrate/). Once the Substrate node is started locally you can proceed to start the relayer.

```
cargo run --bin webb-relayer --features cli -- -c ./config/development/local-substrate -vvv
```

### Run 🏃

Webb Relayer is easy to run and with flexible config 👌. The first step is to create a config file.

Example:

- Create an `.env` file with the following values for the networks you wish to support.
- You also need to request an API key on [etherscan.io](https://docs.etherscan.io/getting-started/viewing-api-usage-statistics) and add it.

```
ETHERSCAN_API_KEY=your-key

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

#### Relayer Common Configuration

| Field           | Description                                                                                      | Optionality |
| --------------- | ------------------------------------------------------------------------------------------------ | ----------- |
| `port`          | Relayer port number                                                                              | Required    |
| `features`      | Enable required features by setting them to `true` . All featured are enabled by default         | Optional    |
| `evm-etherscan` | Etherscan api configuration for chains, required if `private-tx` feature is enabled for relayer. | Optional    |

- `Features` Configuration

```
[features]
governance-relay = true
data-query = true
private-tx-relay = true
```

- `Evm-etherscan` Configuration

```
[evm-etherscan.goerli]
chain-id = 5
api-key = "$ETHERSCAN_GOERLI_API_KEY"
[evm-etherscan.polygon]
chain-id = 137
api-key = "$POLYGONSCAN_MAINNET_API_KEY"
```

#### Chain Configuration

| Field           | Description                                                                                                                        | Optionality            |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ---------------------- |
| `http-endpoint` | Http(s) Endpoint for quick Req/Res. Input can be single http-endpoint or array of multiple http-endpoints.                                                                                                | Required               |
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

| Field                      | Description                                                                              | Optionality |
| -------------------------- | ---------------------------------------------------------------------------------------- | ----------- |
| `contract`                 | Chain contract. Must be either: </br> - VAnchor </br> - SignatureBridge </br>            | Required    |
| `address`                  | The address of this contract on this chain.                                              | Required    |
| `deployed-at`              | The block number where this contract got deployed at.                                    | Required    |
| `events-watcher`           | Control the events watcher for this contract.                                            | Optional    |
| `withdraw-config`          | Config the fees and gas limits of your private transaction relayer.                      | Optional    |
| `proposal-signing-backend` | a value of `ProposalSigingBackend` (for example `{ type = "DKGNode", chain-id = 1080 }`) | Optional    |

#### Event Watcher Configuration

| Field                     | Description                                                                               | Optionality |
| ------------------------- | ----------------------------------------------------------------------------------------- | ----------- |
| `enabled`                 | Boolean value. Default set to `true`                                                      | Optional    |
| `polling-interval`        | Interval between polling next block in millisecond. Default value is `3000ms`             | Optional    |
| `print-progress-interval` | Interval between printing sync progress in millisecond. Default value is `7000ms`         | Optional    |
| `sync-blocks-from`        | Block number from which relayer will start syncing. Default will be `latest` block number | Optional    |

### Docker 🐳

To deploy the relayer with Docker, copy the `docker` folder to your server. Add an `.env` file as described above and save it into the `config` directory. You also need to adjust the `server_name` (domain) specified in `user_conf.d/relayer.conf`. When you are ready, start the relayer with `docker compose up -d`. You can see the logs with `docker compose logs -f`. It will automatically request a TLS certificate using Let's Encrypt and start operating.

> Note: this uses the latest and pre-released version deployed from `develop` branch, change `:edge` to the [latest stable release version](https://github.com/webb-tools/relayer/pkgs/container/relayer). On the other hand if you always want to use the latest development build, set up a cronjob to execute `docker compose pull && docker compose up -d` regularly in the docker folder.

The Docker setup also includes a preconfigured Grafana installation for monitoring. It is available on `localhost:3000` with login `admin` / `admin`. It includes configuration for Slack alerts, to use it enter a Slack Incoming Webhook URL in `provisioning/alerting/alerting.yaml` where it says `slack-placeholder`.



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
