## Relayer Configration files

This directory contains the example configuration files for the relayer. The relayer is configured using a set of TOML file(s) which can
be thought of as a set of blueprints for the relayer. In the following section we will describe the different configuration entries and
how to use them.

### Index

- [Global Configuration](#global-configuration)
  - [port](#global-configuration-port)
  - [features](#global-configuration-features)
    - [governance-relay](#global-configuration-features-governance-relay)
    - [data-query](#global-configuration-features-data-query)
    - [private-tx-relay](#global-configuration-features-private-tx-relay)
- [EVM Chain Configuration](#evm-chain-configuration)
  - [name](#evm-chain-configuration-name)
  - [chain-id](#evm-chain-configuration-chain-id)
  - [http-endpoint](#evm-chain-configuration-http-endpoint)
  - [ws-endpoint](#evm-chain-configuration-ws-endpoint)
  - [private-key](#evm-chain-configuration-private-key)
  - [block-confirmations](#evm-chain-configuration-block-confirmations)
  - [enabled](#evm-chain-configuration-enabled)
  - [explorer](#evm-chain-configuration-explorer)
  - [beneficiary](#evm-chain-configuration-beneficiary)
  - [tx-queue](#evm-chain-configuration-tx-queue)
  - [contracts](#evm-chain-configuration-contracts)
    - [contract](#evm-chain-configuration-contracts-contract)
    - [address](#evm-chain-configuration-contracts-address)
    - [deployed-at](#evm-chain-configuration-contracts-deployed-at)
    - [events-watcher](#evm-chain-configuration-contracts-events-watcher)
      - [enabled](#evm-chain-configuration-contracts-events-watcher-enabled)
      - [enabled-data-query](#evm-chain-configuration-contracts-events-watcher-enabled-data-query)
      - [polling-interval](#evm-chain-configuration-contracts-events-watcher-polling-interval)
      - [max-blocks-per-step](#evm-chain-configuration-contracts-events-watcher-max-blocks-per-step)
      - [sync-blocks-from](#evm-chain-configuration-contracts-events-watcher-sync-blocks-from)
      - [print-progress-interval](#evm-chain-configuration-contracts-events-watcher-print-progress-interval)
    - [proposal-signing-backend](#evm-chain-configuration-contracts-proposal-signing-backend)
      - [type](#evm-chain-configuration-contracts-proposal-signing-backend-type)
      - [node](#evm-chain-configuration-contracts-proposal-signing-backend-node)
      - [private-key](#evm-chain-configuration-contracts-proposal-signing-backend-private-key)
    - [linked-anchors](#evm-chain-configuration-contracts-linked-anchors)
      - [type](#evm-chain-configuration-contracts-linked-anchors-type)
      - [resource-id](#evm-chain-configuration-contracts-linked-anchors-resource-id)
      - [chain-id](#evm-chain-configuration-contracts-linked-anchors-chain-id)
      - [address](#evm-chain-configuration-contracts-linked-anchors-address)
      - [pallet](#evm-chain-configuration-contracts-linked-anchors-pallet)
      - [tree-id](#evm-chain-configuration-contracts-linked-anchors-tree-id)

### Global Configuration

The global configuration file is used to configure the relayer. It is usually located at a file called `main.toml` in the `config` directory.

#### [port](#global-configuration-port)

The port on which the relayer will listen for incoming connections.

- Type: `number`
- Required: `false`
- Default: `9955`
- env: `WEBB_PORT`
- Example:

```toml
port = 9955
```

#### [features](#global-configuration-features)

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

##### [governance-relay](#global-configuration-features-governance-relay)

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

##### [data-query](#global-configuration-features-data-query)

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

##### [private-tx-relay](#global-configuration-features-private-tx-relay)

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

#### [name](#evm-chain-configuration-name)

The name of the chain. This name will be used to identify the chain in the relayer.

- Type: `string`
- Required: `true`
- Example:

```toml
name = "ethereum"
```
