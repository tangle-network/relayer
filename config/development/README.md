## Development Configuration

This directory contains configuration for the development environment.

## EVM Blanknet

The EVM Blanknet is a private Ethereum network that is used for development and testing. It is configured in the `evm-blanknet` directory. This configration starts a 3 chains [Athena, Hermes, and Demeter] each chain will contain the deplpyed contracts and the deployed Anchors will be linked to each other. This will use the Mocked Signing Backend for signing proposals.

## EVM Local <> Tangle

The EVM Local <> Tangle is a private Ethereum network that is used for development and testing. It is configured in the `evm-local-tangle` directory. This configration starts a 3 chains [Athena, Hermes, and Demeter] each chain will contain the deployed contracts and the deployed Anchors will be linked to each other. in addition to that, there is a configured Tangle network that will be used for signing proposals.

## Substrate Local

The Substrate Local is a private Substrate network that is used for development and testing. It is configured in the `substrate-local` directory. This configration will try to connect to a local running Substrate node that uses the Webb Protocol (protocol-substrate) Runtime; for easier testing, you could also use the tangle node for that.

### Running the Development Environment

To run the development environment, you need to have the following installed:

- [Node.js](https://nodejs.org/en/download/)
- [Rust](https://www.rustup.rs/)

Then, you can run the following commands:

```bash
cp config/development/<config>/.env.example .env
```

Compile the relayer crates and Relayer CLI:

```bash
cargo build -p webb-relayer --features cli
```

You will also need the following scripts:

- [LocalEvmVBridge.ts](https://github.com/webb-tools/protocol-solidity/blob/a56f5cd325e7f6b59d2eeae8597836dfae012da5/scripts/evm/deployments/LocalEvmVBridge.ts)
- [Local Standalone Tangle Network\*](https://github.com/webb-tools/tangle/tree/main/scripts#run-a-standalone-tangle-network)

* Note: The Tangle Network is only Required for the EVM Local <> Tangle configuration and is not required for the EVM Blanknet configuration.

After that, you can run the relayer:

```bash
./target/debug/webb-relayer -vvv --tmp --config config/development/<config>
```

## Production Configuration

This Repository does not contain any production ready configration, but they are all examples and for demonstration purposes only.

However, you can take a look at the following configrations:

1. [exclusive-startegies](../exclusive-strategies/) - These are a set of configrations that are used for the exclusive strategies of different roles that you can use the relayer for.
2. [full-support](../full-support/) - Shows you how you can combine the different strategies to support all the roles that you can use the relayer for.
3. [example](../example/) - This is a very simple and minimal configuration for starting a Private Transaction Relayer over the Gorli Testnet.
