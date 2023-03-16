## Relayer Configration files

This directory contains the example configuration files for the relayer. The relayer is configured
using a set of TOML file(s) which can be thought of as a set of blueprints for the relayer. In the
following section we will describe the different configuration entries and how to use them.

### Index

- [Global Configuration](#global-configuration)
  - [port](#port)
  - [features](#features)
    - [governance-relay](#governance-relay)
    - [data-query](#data-query)
    - [private-tx-relay](#private-tx-relay)
- [EVM Chain Configuration](#evm-chain-configuration)
  - [name](#name)
  - [chain-id](#chain-id)
  - [http-endpoint](#http-endpoint)
  - [ws-endpoint](#ws-endpoint)
  - [private-key](#private-key)
  - [block-confirmations](#block-confirmations)
  - [enabled](#enabled)
  - [explorer](#explorer)
  - [beneficiary](#beneficiary)
  - [tx-queue](#tx-queue)
    - [max-sleep-interval](#max-sleep-interval)
  - [contracts](#contracts)
    - [contract](#contract)
    - [address](#address)
    - [deployed-at](#deployed-at)
    - [events-watcher](#events-watcher)
      - [enabled](#enabled-1)
      - [enable-data-query](#enable-data-query)
      - [polling-interval](#polling-interval)
      - [max-blocks-per-step](#max-blocks-per-step)
      - [sync-blocks-from](#sync-blocks-from)
      - [print-progress-interval](#print-progress-interval)
    - [proposal-signing-backend](#proposal-signing-backend)
      - [type](#type)
      - [node](#node)
      - [private-key](#private-key-1)
    - [linked-anchors](#linked-anchors)
      - [type](#type-1)
      - [resource-id](#resource-id)
      - [chain-id](#chain-id-1)
      - [address](#address-1)
      - [pallet](#pallet)
      - [tree-id](#tree-id)
- [Substrate Node Configuration](#substrate-node-configuration)
  - [name](#name-1)
  - [chain-id](#chain-id-2)
  - [http-endpoint](#http-endpoint-1)
  - [ws-endpoint](#ws-endpoint-1)
  - [enabled](#enabled-2)
  - [explorer](#explorer-1)
  - [suri](#suri)
  - [beneficiary](#beneficiary-1)
  - [runtime](#runtime)
  - [tx-queue](#tx-queue-1)
    - [max-sleep-interval](#max-sleep-interval-1)
  - [pallets](#pallets)
    - [pallet](#pallet-1)
    - [events-watcher](#events-watcher-1)
      - [enabled](#enabled-3)
      - [polling-interval](#polling-interval-1)
      - [max-blocks-per-step](#max-blocks-per-step-1)
      - [sync-blocks-from](#sync-blocks-from-1)
      - [print-progress-interval](#print-progress-interval-1)
      - [enable-data-query](#enable-data-query-1)
    - [proposal-signing-backend](#proposal-signing-backend-1)
      - [type](#type)
      - [node](#node)
      - [private-key](#private-key-1)
    - [linked-anchors](#linked-anchors)
      - [type](#type-1)
      - [resource-id](#resource-id)
      - [chain-id](#chain-id-2)
      - [address](#address-1)
      - [pallet](#pallet-1)
      - [tree-id](#tree-id)

### Global Configuration

The global configuration file is used to configure the relayer. It is usually located at a file
called `main.toml` in the `config` directory.

#### port

The port on which the relayer will listen for incoming connections.

- Type: `number`
- Required: `false`
- Default: `9955`
- env: `WEBB_PORT`
- Example:

```toml
port = 9955
```

#### features

The features section is used to enable or disable the relayer features.

- Type: `table`
- Required: `false`
- Default: `{ governance-relay = true, data-query = true, private-tx-relay = true }`
- env: `WEBB_FEATURES_GOVERNANCE_RELAY`, `WEBB_FEATURES_DATA_QUERY`,
  `WEBB_FEATURES_PRIVATE_TX_RELAY`
- Example:

```toml
[features]
governance-relay = true
data-query = true
private-tx-relay = true
```

##### governance-relay

Enable or disable the governance-relay feature. Enabling this feature will allow the relayer to
relay proposals and vote on them between the chains.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_FEATURES_GOVERNANCE_RELAY`
- Example:

```toml
[features]
governance-relay = true
```

##### data-query

Enable or disable the data-query feature. Enabling this feature will allow the relayer to work as a
data query oracle.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_FEATURES_DATA_QUERY`
- Example:

```toml
[features]
data-query = true
```

##### private-tx-relay

Enable or disable the private-tx-relay feature. When enabled, the relayer will be able to relay private transactions.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_FEATURES_PRIVATE_TX_RELAY`
- Example:

```toml
[features]
private-tx-relay = true
```

### EVM Chain Configuration

The EVM chain configuration file is used to configure the relayer to work with a specific EVM chain.
It is usually located at a file called `evm/<chain-name>.toml` in the `config` directory.

The value of this configration is a table, and the name of the table is the name of the chain, for
example:

```toml
[evm.ethereum]
chain-id = 1
name = "ethereum"
# ...
```

So, in general it is `[evm.<chain-name>]`, where `<chain-name>` is the name of the chain. The
following sections describe the different configuration entries and how to use them.

#### name

The name of the chain. This name will be used to identify the chain in the relayer.

- Type: `string`
- Required: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_NAME`
- Example:

```toml
name = "ethereum"
```

#### chain-id

The chain id of the chain. This id will be used to identify the chain in the relayer.

- Type: `number`
- Required: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_CHAIN_ID`
- Example:

```toml
chain-id = 1
```

#### http-endpoint

The HTTP(s) RPC endpoint for this chain, used for watching events, and sending transactions.

- Type: `string`
- Required: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_HTTP_ENDPOINT`
- Example:

```toml
http-endpoint = "https://mainnet.infura.io/v3/<project-id>"
```

#### ws-endpoint

The WebSocket RPC endpoint for this chain, used for watching events, and sending transactions.

- Type: `string`
- Required: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_WS_ENDPOINT`
- Example:

```toml
ws-endpoint = "wss://mainnet.infura.io/ws/v3/<project-id>"
```

#### private-key

The private key configuration specifies the private key of the account on the EVM chain used for signing transactions. The format of the private key depends on its value:

If the private key starts with `0x`, it is considered a raw (64 bytes) hex-encoded private key. Example: `0x8917174396171783496173419137618235192359106130478137647163400318`.

If the private key starts with `$`, it is considered an environment variable containing a hex-encoded private key. Example: `$MAINNET_PRIVATE_KEY`.

If the private key starts with '> ', it is considered a command that the relayer will execute to obtain the hex-encoded private key. Example: > `./getKey.sh mainnet-privatekey`.

If the private key doesn't contain special characters and has 12 or 24 words, it is considered a mnemonic string. The words should be separated by spaces, and the string should not be enclosed in quotes or any other characters.

- Type: `string`
- Required:
  - `true` if `features.governance-relay` is `true`
  - `true` if `features.private-tx-relay` is `true`
  - `false` otherwise
- env: `WEBB_EVM_<CHAIN_NAME>_PRIVATE_KEY`
- Example:

```toml
private-key = "0x8917174396171783496173419137618235192359106130478137647163400318"
```

> **Warning** The private key should be kept secret, and should not be hard-coded in the
> configuration file. Instead, it should be loaded from an environment variable, or a file.

#### block-confirmations

The number of block confirmations to wait before processing an event.

- Type: `number`
- Required: `false`
- Default: `0`
- env: `WEBB_EVM_<CHAIN_NAME>_BLOCK_CONFIRMATIONS`
- Example:

```toml
block-confirmations = 5
```

#### enabled

Enable or disable this chain. If this is set to `false`, then the relayer will not consider this
chain while loading the configuration files.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_ENABLED`
- Example:

```toml
enabled = true
```

#### explorer

The block explorer URL for this chain. This is used to generate links to the transactions, useful
for debugging.

- Type: `string`
- Required: `false`
- Default: `null`
- env: `WEBB_EVM_<CHAIN_NAME>_EXPLORER`
- Example:

```toml
explorer = "https://etherscan.io"
```

#### beneficiary

The address of the beneficiary account on this chain. This is used to receive the fees from relaying
transactions. It is optional, and if not provided, the relayer will use the account address of the
provided [private-key](#private-key) for this chain.

- Type: `string`
- Required: `false`
- Default: `null`
- env: `WEBB_EVM_<CHAIN_NAME>_BENEFICIARY`
- Example:

```toml
beneficiary = "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045"
```

#### Tx Queue

The tx queue is used to store the transactions that are waiting to be sent to the chain. The relayer
uses a database to store the transactions, and the configuration for the database is stored in the
`tx-queue` section of the configuration file.

##### max-sleep-interval

The maximum time to sleep between sending transactions. This controls the rate at which the relayer sends transactions to the chain.

- Type: `number`
- Required: `false`
- Default: `10000ms`
- env: `WEBB_EVM_<CHAIN_NAME>_TX_QUEUE_MAX_SLEEP_INTERVAL`
- Example:

```toml
tx-queue = { max-sleep-interval = 5000 }
```

#### Contracts

The contracts section is used to configure the contracts that the relayer will use to interact with
the chain. The contracts are identified by their address and type, and the configuration for each
contract is stored in a list, each has a different type and address.

Here is an example of the configuration for the contracts:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
address = "0x8eB24319393716668D768dCEC29356ae9CfFe285"
deployed-at = 3123412
# ...

[[evm.ethereum.contracts]]
contract = "SignatureBridge"
address = "0xd8dA6BF26964aF9D7eEd9e03E03517D3faA9d045"
deployed-at = 3123413
# ...
```

##### contract

The type of the contract. This is used to identify the contract, and the relayer will use this to
determine which contract to use for a specific operation. Each contract type has its own
configuration, and different internal service that will handle the contract operations.

- Type: `enum`
- Possible values:
  - `VAnchor`
  - `OpenVAnchor`
  - `SignatureBridge`
- Required: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_CONTRACT`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
```

##### address

The address of the contract on the configured chain.

- Type: `string`
- Required: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_ADDRESS`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
address = "0x8eB24319393716668D768dCEC29356ae9CfFe285"
```

##### deployed-at

The block number at which the contract was deployed. This is used to determine the starting block number for scanning events.

- Type: `number`
- Required: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_DEPLOYED_AT`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
deployed-at = 3123412
```

##### Events Watcher

The events watcher is used to watch for events emitted by the contracts. The relayer uses this
configration values to determine how the relayer will poll the events from that contract.

- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
events-watcher = { enabled = true, poll-interval = 12000 }
```

###### enabled

Enable or disable the events watcher for this contract.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_EVENTS_WATCHER_ENABLED`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
events-watcher = { enabled = true }
```

###### enable-data-query

Enable or disable the data query for this contract. When enabled, it allows the relayer to query the contract's events, such as the leaves.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_EVENTS_WATCHER_ENABLE_DATA_QUERY`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
events-watcher = { enable-data-query = true }
```

###### poll-interval

The interval at which the relayer will poll for events from the contract.

- Type: `number`
- Required: `false`
- Default: `7000ms`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_EVENTS_WATCHER_POLL_INTERVAL`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
events-watcher = { poll-interval = 12000 }
```

##### max-blocks-per-step

The maximum number of blocks to scan for events in a single step. This controls the rate at which the relayer scans for events from the contract, and also affects the speed at which the relayer synchronizes with the chain.

- Type: `number`
- Required: `false`
- Default: `100`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_MAX_BLOCKS_PER_STEP`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
events-watcher = { max-blocks-per-step = 500 }
```

##### sync-blocks-from

The block number to start scanning for events from. This value can override the [deployed-block](#deployed-at) configuration value but is only used during the initial synchronization of the chain. If not specified, the relayer uses the [deployed-block](#deployed-at) configuration value as the default starting block number.

- Type: `number`
- Required: `false`
- Default: `null` (_will use the value of the [deployed-at](#deployed-at) configuration value_)
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_SYNC_BLOCKS_FROM`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
events-watcher = { sync-blocks-from = 3123412 }
```

##### print-progress-interval

The interval at which the relayer will print the progress of the syncing process. This is used to
show the user the progress of the syncing process which could be useful for debugging purposes.

- Type: `number`
- Required: `false`
- Default: `30000ms`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_PRINT_PROGRESS_INTERVAL`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
events-watcher = { print-progress-interval = 60000 }
```

##### Proposal Signing Backend

A Proposal Signing backend is used for signing proposals that the relayer will submit to be signed
and later executed on the target chain. Currently, there are two types of proposal signing backends,
the Mocked one, and the DKG based one.

###### type

The type of the proposal signing backend to use.

- Type: `string`
- Required: `true`
- Possible values:
  - `Mocked`
  - `Dkg`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_PROPOSAL_SIGNING_BACKEND_TYPE`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
proposal-signing-backend = { type = "Mocked", private-key = "0x..." }
```

Let's take a look at each one of them.

###### Mocked Proposal Signing Backend

The mocked proposal signing backend is used for testing purposes, and it is mostly used for our
local and integration tests. This backend will sign the proposals with a mocked keypair; hence
should not be used in production environment.

- Available configuration values:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
proposal-signing-backend = { type = "Mocked", private-key = "0x..." }
```

###### private-key

The private key to use for signing the proposals. Only used by the mocked proposal signing backend.

- Type: `string`
- Required:
  - `true` if the [type](#type) is `Mocked`
  - `false` otherwise
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_PROPOSAL_SIGNING_BACKEND_PRIVATE_KEY`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
proposal-signing-backend = { type = "Mocked", private-key = "0x..." }
```

###### Dkg Proposal Signing Backend

The DKG proposal signing backend is used for signing the proposals using the DKG configured node.

- Available configuration values:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
proposal-signing-backend = { type = "Dkg", node = "1080" }
```

###### node

The node's chain-id to use for signing the proposals. Only used by the DKG proposal signing backend.

- Type: `string`
- Required:
  - `true` if the [type](#type) is `Dkg`
  - `false` otherwise
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_PROPOSAL_SIGNING_BACKEND_NODE`
- Example:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
proposal-signing-backend = { type = "Dkg", node = "1080" }
```

##### Linked Anchors

The Linked Anchors configuration is used to define the linked anchors for the VAnchor contract. This configuration is only available when the [type](#type) is set to `VAnchor`.

The configuration value is a list of Linked Anchors, defined in a human-readable format. However, the relayer will convert them to a raw format before using them.

- Required: `false`
- Default: `null` (defaults to an empty list)
- Available configuration values:

```toml
[[evm.ethereum.contracts]]
contract = "VAnchor"
linked-anchors = [
  { type = "Evm", chain-id = 1, address = "0x..." },
  { type = "Substrate", chain-id = 1080, pallet = 42, tree-id = 4 },
  { type = "Raw", resource-id = "0x..." },
]
```

###### type

The type of the linked anchor definition.

- Type: `string`
- Required: `true`
- Possible values:
  - `Evm`
  - `Substrate`
  - `Raw`
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_LINKED_ANCHORS_<INDEX>_TYPE`

###### chain-id

The chain-id of the linked anchor definition. Only used by the `Evm` and `Substrate` types.

- Type: `number`
- Required:
  - `true` if the [type](#type) is `Evm` or `Substrate`
  - `false` otherwise
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_LINKED_ANCHORS_<INDEX>_CHAIN_ID`
- Example:

```toml
[[evm.ethereum.contracts]]
type = "VAnchor"
linked-anchors = [
  { type = "Evm", chain-id = 1, address = "0x..." },
  { type = "Substrate", chain-id = 1080, pallet = 42, tree-id = 4 },
]
```

###### address

The address of the linked anchor definition. Only used by the `Evm` type.

- Type: `string`
- Required:
  - `true` if the [type](#type) is `Evm`
  - `false` otherwise
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_LINKED_ANCHORS_<INDEX>_ADDRESS`
- Example:

```toml
[[evm.ethereum.contracts]]
type = "VAnchor"
linked-anchors = [
  { type = "Evm", chain-id = 1, address = "0x..." },
]
```

###### pallet

The pallet of the linked anchor definition. Only used by the `Substrate` type.

- Type: `number`
- Required:
  - `true` if the [type](#type) is `Substrate`
  - `false` otherwise
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_LINKED_ANCHORS_<INDEX>_PALLET`
- Example:

```toml
[[evm.ethereum.contracts]]
type = "VAnchor"
linked-anchors = [
  { type = "Substrate", chain-id = 1080, pallet = 42, tree-id = 4 },
]
```

###### tree-id

The tree-id of the linked anchor definition. Only used by the `Substrate` type.

- Type: `number`
- Required:
  - `true` if the [type](#type) is `Substrate`
  - `false` otherwise
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_LINKED_ANCHORS_<INDEX>_TREE_ID`
- Example:

```toml
[[evm.ethereum.contracts]]
type = "VAnchor"
linked-anchors = [
  { type = "Substrate", chain-id = 1080, pallet = 42, tree-id = 4 },
]
```

###### resource-id

The resource-id of the linked anchor definition. Only used by the `Raw` type.

- Type: `string`
- Required:
  - `true` if the [type](#type) is `Raw`
  - `false` otherwise
- env: `WEBB_EVM_<CHAIN_NAME>_CONTRACTS_<INDEX>_LINKED_ANCHORS_<INDEX>_RESOURCE_ID`
- Example:

```toml
[[evm.ethereum.contracts]]
type = "VAnchor"
linked-anchors = [
  { type = "Raw", resource-id = "0x..." },
]
```

### Substrate Node Configuration

The Substrate Node configuration file is used to specify the configuration settings required for the relayer to work with a specific Substrate Node. The file is typically located in the `config` directory and named `substrate/<node-name>.toml`.

The configuration value is a table, and its name corresponds to the name of the node. For example:

```toml
[substrate.tangle]
chain-id = 1080
name = "tangle"
# ...
```

In general, the configuration table for a node is identified as `[substrate.<node-name>]`, where `<node-name>` is the name of the network. The following sections outline the different configuration entries and how to use them.

#### name

The name of the Substrate node.

- Type: `string`
- Required: `true`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_NAME`
- Example:

```toml
[substrate.tangle]
name = "tangle"
```

#### chain-id

The chain-id of the Substrate node.

- Type: `number`
- Required: `true`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_CHAIN_ID`
- Example:

```toml
[substrate.tangle]
chain-id = 1080
```

#### http-endpoint

The RPC endpoint of the Substrate node.

- Type: `string`
- Required: `true`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_HTTP_ENDPOINT`
- Example:

```toml
[substrate.tangle]
http-endpoint = "http://localhost:9933"
```

#### ws-endpoint

The RPC WebSocket endpoint of the Substrate node.

- Type: `string`
- Required: `true`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_WS_ENDPOINT`
- Example:

```toml
[substrate.tangle]
ws-endpoint = "ws://localhost:9944"
```

#### runtime

The runtime of the running substrate node. These are predefined in the relayer, with each having
different types of Options.

- Type: `string`
- Required: `true`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_RUNTIME`
- Possible values:
  - `DKG` (that is, `dkg-substrate`)
  - `WebbProtocol` (also known as `protocol-substrate`)
- Example:

```toml
[substrate.tangle]
runtime = "WebbProtocol"
```

#### enabled

Whether the Substrate node is enabled or not. If it is not enabled, the relayer will not try to add
it to the list of available nodes.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_ENABLED`
- Example:

```toml
[substrate.tangle]
enabled = true
```

#### explorer

The explorer of the Substrate Network. This is used to display clickable links to the explorer in
the logs.

- Type: `string`
- Required: `false`
- Default: `null`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_EXPLORER`
- Example:

```toml
[substrate.tangle]
explorer = "https://tangle-explorer.webb.tools"
```

#### suri

SURI stands for "secret URI". It is a mnemonic phrase that can be used to generate a private key.
This is used to sign extrinsics. We will refer to this as the "s" for now. The value is a string
(`s`) that we will try to interpret the string in order to generate a key pair. In the case that the
pair can be expressed as a direct derivation from a seed (some cases, such as Sr25519 derivations
with path components cannot).

This takes a helper function to do the key generation from a phrase, password and junction iterator.

- If `s` begins with a `$` character it is interpreted as an environment variable.
- If `s` is a possibly `0x` prefixed 64-digit hex string, then it will be interpreted directly as a
  `MiniSecretKey` (aka "seed" in `subkey`).
- If `s` is a valid BIP-39 key phrase of 12, 15, 18, 21 or 24 words, then the key will be derived
  from it. In this case:
  - the phrase may be followed by one or more items delimited by `/` characters.
  - the path may be followed by `///`, in which case everything after the `///` is treated as a
    password.
- If `s` begins with a `/` character it is prefixed with the Substrate public `DEV_PHRASE` and
  interpreted as above.

In this case they are interpreted as HDKD junctions; purely numeric items are interpreted as
integers, non-numeric items as strings. Junctions prefixed with `/` are interpreted as soft
junctions, and with `//` as hard junctions.

There is no correspondence mapping between SURI strings and the keys they represent. Two different
non-identical strings can actually lead to the same secret being derived. Notably, integer junction
indices may be legally prefixed with arbitrary number of zeros. Similarly an empty password (ending
the SURI with `///`) is perfectly valid and will generally be equivalent to no password at all.

The value of this string could also start with `$` to indicate that it is an environment variable,
in which case the value of the environment variable will be used.

> **Warning**: This is a sensitive value, and should be kept secret. It is recommended to use an
> environment variable to store the value of this string.

- Type: `string`
- Required: `true`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_SURI`
- Example:

```toml
[substrate.tangle]
suri = "$TANGLE_SURI"
```

#### beneficiary

The beneficiary is the address that will receive the fees from the transactions. This is optional,
and will default to the address derived from the `suri` if not provided.

- Type: `string`
- Required: `false`
- Default: `null`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_BENEFICIARY`
- Example:

```toml
[substrate.tangle]
beneficiary = "5FZ2Wfjy5rZ7g5j7Y9Zwv5Z4Z3Z9Z9Z9Z9Z9Z9Z9Z9Z9Z9Z9Z9"
```

#### Tx Queue

The transaction queue is a queue of transactions that are waiting to be sent to the Substrate node.

##### max-sleep-interval

The maximum sleep interval between sending transactions to the Substrate node.

- Type: `number`
- Required: `false`
- Default: `10000ms`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_TRANSACTION_QUEUE_MAX_SLEEP_INTERVAL`
- Example:

```toml
[substrate.tangle]
tx-queue = { max-sleep-interval = 10000 }
```

#### Pallets

The pallets are the different pallets that are used by the relayer. Each will define its own
configration which will eventually be used by the relayer to start a different service for each
pallet.

There are currently 5 different pallets that are supported by the relayer:

- `DKG`
- `DkgProposals`
- `DkgProposalHandler`
- `SignatureBridge`
- `VAnchorBn254`

- Type: `table`
- Required: `false`
- Default: `null` (empty table)
- Example:

```toml
[[substrate.tangle.pallets]]
pallet = "DKG"

[[substrate.tangle.pallets]]
pallet = "DkgProposals"
# ...
```

##### pallet

The type of the pallet. This is used to determine which pallet to use.

- Type: `string`
- Required: `true`
- Possible values:
  - `DKG`
  - `DkgProposals`
  - `DkgProposalHandler`
  - `SignatureBridge`
  - `VAnchorBn254`
- Example:

```toml
[[substrate.tangle.pallets]]
pallet = "DKG"
```

##### events-watcher

The events watcher is used to watch for events emitted by the pallet.

- Type: `table`
- Required: `false`
- Default: `null` (empty table)
- Example:

```toml
[[substrate.tangle.pallets]]
pallet = "DKG"
events-watcher = { enabled = true }
```

###### enabled

Whether the events watcher is enabled or not. If it is not enabled, the relayer will not try to
watch events emitted by the pallet.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_PALLET_<INDEX>_EVENTS_WATCHER_ENABLED`
- Example:

```toml
[[substrate.tangle.pallets]]
pallet = "DKG"
events-watcher = { enabled = true }
```

##### enable-data-query

Whether the data query is enabled or not. If it is not enabled, the relayer will not try to query
the data from the pallet.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_PALLET_<INDEX>_EVENTS_WATCHER_ENABLE_DATA_QUERY`
- Example:

```toml
[[substrate.tangle.pallets]]
pallet = "DKG"
events-watcher = { enabled = true, enable-data-query = true }
```

##### polling-interval

The polling interval is the interval at which the relayer will poll the Substrate node for new
blocks.

- Type: `number`
- Required: `false`
- Default: `6000ms`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_PALLET_<INDEX>_EVENTS_WATCHER_POLLING_INTERVAL`
- Example:

```toml
[[substrate.tangle.pallets]]
pallet = "DKG"
events-watcher = { enabled = true, polling-interval = 6000 }
```

##### max-blocks-per-step

The maximum number of blocks to process per step.

- Type: `number`
- Required: `false`
- Default: `100`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_PALLET_<INDEX>_EVENTS_WATCHER_MAX_BLOCKS_PER_STEP`
- Example:

```toml
[[substrate.tangle.pallets]]
pallet = "DKG"
events-watcher = { enabled = true, max-blocks-per-step = 100 }
```

##### sync-blocks-from

The block number from which to start syncing events from. This is useful if you want to start the
relayer from a specific block instead of block zero.

- Type: `number`
- Required: `false`
- Default: `0`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_PALLET_<INDEX>_EVENTS_WATCHER_SYNC_BLOCKS_FROM`
- Example:

```toml
[[substrate.tangle.pallets]]
pallet = "DKG"
events-watcher = { enabled = true, sync-blocks-from = 42069 }
```

##### print-progress-interval

The interval at which the relayer will print the progress of the syncing process. Useful for
debugging.

- Type: `number`
- Required: `false`
- Default: `12000ms`
- env: `WEBB_SUBSTRATE_<NODE_NAME>_PALLET_<INDEX>_EVENTS_WATCHER_PRINT_PROGRESS_INTERVAL`
- Example:

```toml
[[substrate.tangle.pallets]]
pallet = "DKG"
events-watcher = { enabled = true, print-progress-interval = 12000 }
```
