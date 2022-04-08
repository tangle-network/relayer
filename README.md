<h1 align="center">Webb Relayer 🕸️ </h1>
<div align="center">
<a href="https://www.webb.tools/">
    <img alt="Webb Logo" src="./assets/webb-icon.svg" width="15%" height="30%" />
  </a>
  </div>
<p align="center">
    <strong>🚀  The Webb Relayer  🧑‍✈️</strong>
    <br />
    <sub> ⚠️ Beta Software ⚠️ </sub>
</p>

<div align="center" >

[![GitHub release (latest by date)](https://img.shields.io/github/v/release/webb-tools/relayer?style=flat-square)](https://github.com/webb-tools/relayer/releases/latest)
[![GitHub Workflow Status](https://img.shields.io/github/workflow/status/webb-tools/relayer/CI?style=flat-square)](https://github.com/webb-tools/relayer/actions)
[![Codecov](https://img.shields.io/codecov/c/gh/webb-tools/relayer?style=flat-square&token=AFS375VWRS)](https://codecov.io/gh/webb-tools/relayer)
[![License Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=flat-square)](https://opensource.org/licenses/Apache-2.0)
[![Twitter](https://img.shields.io/twitter/follow/webbprotocol.svg?style=flat-square&label=Twitter&color=1DA1F2)](https://twitter.com/webbprotocol)
[![Telegram](https://img.shields.io/badge/Telegram-gray?logo=telegram)](https://t.me/webbprotocol)
[![Discord](https://img.shields.io/discord/833784453251596298.svg?style=flat-square&label=Discord&logo=discord)](https://discord.gg/cv8EfJu3Tn)

</div>

<!-- TABLE OF CONTENTS -->
<h2 id="table-of-contents"> 📖 Table of Contents</h2>

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

In the Webb Protocol, the relayer is a multi-faceted oracle, data relayer, and protocol governance participant. Relayers fulfill the role of an oracle where the external data sources that they listen to are the state of the anchors for a bridge. Relayers, as their name entails, relay information for a connected set of Anchors on a bridge. This information is then used to update the state of each Anchor and allow applications to reference, both privately and potentially not, properties of data stored across the other connected Anchors.

The relayer system is composed of three main components. Each of these components should be thought of as entirely separate because they could be handled by different entities entirely.

1. Private transaction relaying (of user bridge transactions like Tornado Cash’s relayer)
2. Data querying (for zero-knowledge proof generation)
3. Data proposing and signature relaying (of DKG proposals)

For additional information, please refer to the [Webb Relayer Rust Docs](https://webb-tools.github.io/relayer/) 📝. Have feedback on how to improve the relayer network? Or have a specific question to ask? Checkout the [Relayer Feedback Discussion](https://github.com/webb-tools/feedback/discussions/categories/webb-relayer-feedback) 💬.

### Top-level directory layout

```
src/
  |____tx_queue.rs          # A queue for orderly handling of transactions.
  |____handler.rs           # Logic for what to do when a client is interacting with this relayer.
  |____config.rs            # Functionality related to parsing of configurable values.
  |____events_watcher       # Sync to different network types (EVM, Substrate), and act on different events.
  |____service.rs           # The entry for tasks once the relayer is operating.
  |____main.rs              # Build and start the relayer.
  |____probe.rs             # Debugging relayer lifecycle, sync state, or other relayer state.
  |____utils.rs             # Common functionality.
  |____context.rs           # Access the parsed configuration and generate providers and wallets.
  |____store                # Logic for storing information with different backends.

```

### Prerequisites

This repo uses Rust so it is required to have a Rust developer environment set up. First install and configure rustup:

```bash
# Install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Configure
source ~/.cargo/env
```

Configure the Rust toolchain to default to the latest stable version, add nightly:

```bash
rustup default stable
rustup update
rustup update nightly
```

Great! Now your Rust environment is ready! 🚀🚀

### Installation 💻

#### Unix (Linux, macOS, WSL2, ..)

```
git clone https://github.com/webb-tools/relayer.git

cargo build --release
```

<h2 id="usage"> Usage </h2>

### Quick Start ⚡

#### Local EVM Tornado

Eager to try out the Webb Relayer and see it in action? Run a relayer with our preset EVM tornado configuration to get up and running immediately.

```
./target/release/webb-relayer -c config/config-tornados/ethereum -vv
```

> Hot Tip 🌶️: To increase the logger verbosity add additional `-vvvv` during start up command. You will now see `TRACE` logs. Happy debugging!

#### Local Substrate Mixer

To use the relayer for our Substrate mixer, you will first need to start a local substrate node that integrates with our pallets [webb-standalone-node](https://github.com/webb-tools/protocol-substrate/). Once the Substrate node is started locally you can proceed to start the relayer.

```
 ./target/release/webb-relayer -c config/local-substrate -vv
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

The table below documents all the configuration options available for both chain and contract set ups. For a completed example, check out [Harmony's testnet configuration](./config/config-tornados/harmony/testnet1.toml).

**Note:** You can also review the different chain configurations for EVM and Substrate.

- [`SubstrateConfig`](https://docs.webb.tools/relayer/webb_relayer/config/struct.SubstrateConfig.html)
- [`EvmChainConfig`](https://docs.webb.tools/relayer/webb_relayer/config/struct.EvmChainConfig.html)

#### Chain Configuration

| Field           | Description                                                                                                                        | Optionality            |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ---------------------- |
| `http-endpoint` | Http(s) Endpoint for quick Req/Res                                                                                                 | Required               |
| `ws-endpoint`   | Websocket Endpoint for long living connections                                                                                     | Required               |
| `explorer`      | Block explorer, used for generating clickable links for transactions that happens on this chain.                                   | Optional               |
| `chain-id`      | Chain specific id.                                                                                                                 | Required               |
| `private-key`   | The Private Key of this account on this network. See [PrivateKey Docs for secure setup]()                                          | Required               |
| `beneficiary`   | The address of the account that will receive relayer fees.                                                                         | Optional               |
| `runtime`       | Indicates Substrate runtime to use                                                                                                 | Required for Substrate |
| `suri`          | Interprets a string in order to generate a key Pair. In the case that the pair can be expressed as a direct derivation from a seed | Required for Substrate |
| `pallets`       | Supported pallets for a particular Substrate node                                                                                  | Optional               |

#### Contract Configuration

| Field                     | Description                                                                                                                                                   | Optionality |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- |
| `contract`                | Chain contract. Must be either: </br> - Anchor (tornado protocol) </br> - Anchor2 (darkwebb protocol) </br> - SignatureBridge </br> - GovernanceBravoDelegate | Required    |
| `address`                 | The address of this contract on this chain.                                                                                                                   | Required    |
| `deployed-at`             | The block number where this contract got deployed at.                                                                                                         | Required    |
| `size`                    | The size of this contract. **Note**: only available for `Anchor` and `Anchor2` contracts.                                                                     | Optional    |
| `events-watcher`          | Control the events watcher for this contract.                                                                                                                 | Optional    |
| `withdraw-fee-percentage` | The fee percentage that your account will receive when you relay a transaction over this chain.                                                               | Optional    |
| `withdraw-gaslimit`       | A hex value of the gaslimit when doing a withdraw relay transaction on this chain.                                                                            | Optional    |

### Docker 🐳

To use Docker to run the relayer, you will need to specify a config file and provide an `.env` file as described above. Then proceed to save it into the `config` directory.

To run docker image:

```sh
docker run --rm -v "<ABSOLUTE_PATH_TO_CONFIGS_DIRECTORY>:/config" --env-file .env -p 9955:9955 ghcr.io/webb-tools/relayer:edge
```

> Note: this uses the latest and pre-released version deployed from `main` branch, change `edge` to the latest stable release version.

This will mount a configuration files at the `/config` directory inside the container so it would allow it to read the configuration you added.

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
                "contract": "Tornado",
                "address": "0x626fec5ffa7bf1ee8ced7dabde545630473e3abb",
                "deployedAt": 8896800,
                "eventsWatcher": {
                    "enabled": true,
                    "pollingInterval": 15000
                },
                "size": 0.1,
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

- `chain_id`
- `contract address`

```
/api/v1/leaves/4/0x626fec5ffa7bf1ee8ced7dabde545630473e3abb
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

<h2 id="test"> Testing 🧪 </h2>

The following instructions outlines how to run the relayer base test suite and E2E test suite.

### To run base tests

```
cargo test
```

### To run E2E tests

1. Run `cargo build --release --features integration-tests`
2. Run `cd tests && git submodule update --init --recursive`
3. Run `yarn install` (in `tests` dir)
4. `yarn test`

## Contributing

Interested in contributing to the Webb Relayer Network? Thank you so much for your interest! We are always appreciative for contributions from the open-source community!

If you have a contribution in mind, please check out our [Contribution Guide](./.github/CONTRIBUTING.md) for information on how to do so. We are excited for your first contribution!

## License

Licensed under <a href="LICENSE">Apache 2.0 license</a>.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache 2.0 license, shall
be licensed as above, without any additional terms or conditions.
