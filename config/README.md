## Relayer Configration files

This directory contains the example configuration files for the relayer. The relayer is configured using a set of TOML file(s) which can
be thought of as a set of blueprints for the relayer. In the following section we will describe the different configuration entries and
how to use them.

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

### Global Configuration

The global configuration file is used to configure the relayer. It is usually located at a file called `main.toml` in the `config` directory.

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
- env: `WEBB_FEATURES_GOVERNANCE_RELAY`, `WEBB_FEATURES_DATA_QUERY`, `WEBB_FEATURES_PRIVATE_TX_RELAY`
- Example:

```toml
[features]
governance-relay = true
data-query = true
private-tx-relay = true
```

##### governance-relay

Enable or disable the governance-relay feature. Enabling this feature will allow the relayer to relay proposals and votes on them
between the chains.

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

Enable or disable the data-query feature. Enabling this feature will allow the relayer to work as a data query oracle.

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

Enable or disable the private-tx-relay feature. Enabling this feature will allow the relayer to relay private transactions, to preserve the privacy of the transactions.

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

The EVM chain configuration file is used to configure the relayer to work with a specific EVM chain. It is usually located at a file called `evm/<chain-name>.toml` in the `config` directory.

The value of this configration is a table, and the name of the table is the name of the chain, for example:

```toml
[evm.ethereum]
chain-id = 1
name = "ethereum"
# ...
```

So, in general it is `[evm.<chain-name>]`, where `<chain-name>` is the name of the chain. The following sections describe the different configuration entries and how to use them.

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

The Private Key of this account on this network, used for signing transactions.
the format is more dynamic here:

1.if it starts with '0x' then this would be raw (64 bytes) hex encoded private key.
Example: `0x8917174396171783496173419137618235192359106130478137647163400318`

2.if it starts with '$' then it would be considered as an Enviroment variable of a hex-encoded private key.
  Example: `$MAINNET_PRIVATE_KEY`

3.if it starts with '> ' then it would be considered as a command that the relayer would execute
and the output of this command would be the hex encoded private key.
Example: `> ./getKey.sh mainnet-privatekey`

4.if it doesn't contains special characters and has 12 or 24 words in it
then we should process it as a mnemonic string: 'word two three four ...'

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

> **Warning**
> The private key should be kept secret, and should not be hard-coded in the configuration file. Instead, it should be loaded from an environment variable, or a file.

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

Enable or disable this chain. If this is set to `false`, then the relayer will not consider this chain while loading the configuration files.

- Type: `bool`
- Required: `false`
- Default: `true`
- env: `WEBB_EVM_<CHAIN_NAME>_ENABLED`
- Example:

```toml
enabled = true
```

#### explorer

The block explorer URL for this chain. This is used to generate links to the transactions, useful for debugging.

- Type: `string`
- Required: `false`
- Default: `null`
- env: `WEBB_EVM_<CHAIN_NAME>_EXPLORER`
- Example:

```toml
explorer = "https://etherscan.io"
```

#### beneficiary

The address of the beneficiary account on this chain. This is used to receive the fees from relaying transactions.
It is optional, and if not provided, the relayer will use the account address of the proivided [private-key](#private-key) for this chain.

- Type: `string`
- Required: `false`
- Default: `null`
- env: `WEBB_EVM_<CHAIN_NAME>_BENEFICIARY`
- Example:

```toml
beneficiary = "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045"
```

#### Tx Queue

The tx queue is used to store the transactions that are waiting to be sent to the chain. The relayer uses a database to store the transactions, and the configuration for the database is stored in the `tx-queue` section of the configuration file.

##### max-sleep-interval

The maximum time to sleep between sending transactions. This to control the rate at which the relayer sends transactions to the chain.

- Type: `number`
- Required: `false`
- Default: `10000ms`
- env: `WEBB_EVM_<CHAIN_NAME>_TX_QUEUE_MAX_SLEEP_INTERVAL`
- Example:

```toml
tx-queue = { max-sleep-interval = 5000 }
```

#### Contracts
