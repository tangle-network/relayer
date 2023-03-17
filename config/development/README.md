## Development Configuration

This directory contains configuration for the development environment.

## EVM Blanknet

The EVM Blanknet is a private Ethereum network that is used for development and testing. It is
configured in the `evm-blanknet` directory. This configuration uses 3 chains [Athena, Hermes, and
Demeter] each chain contains the deplpyed contracts and the deployed Anchors will be linked to
each other. This will use the Mocked Signing Backend for signing proposals.

## EVM Local <> Tangle

The EVM Local <> Tangle is a private Ethereum network that is used for development and testing. It
is configured in the `evm-local-tangle` directory. This configuration starts a 3 chains [Athena,
Hermes, and Demeter] each chain will contain the deployed contracts and the deployed Anchors will be
linked to each other. in addition to that, there is a configured Tangle network that will be used
for signing proposals.

## Substrate Local

The Substrate Local is a private Substrate network that is used for development and testing. It is
configured in the `substrate-local` directory. This configuration will try to connect to a local
running Substrate node that uses the Webb Protocol (protocol-substrate) Runtime; for easier testing,
you could also use the tangle node for that.

### Running the Development Environment

To run the development environment, you need to have the following installed:

- [Node.js](https://nodejs.org/en/download/)
- [Rust](https://www.rustup.rs/)

Then, you can run the following commands:

```bash
cp config/development/<config>/.env.example .env
```

You will also need to follow these steps:

- [Running Local Bridge](https://github.com/webb-tools/webb-dapp/tree/develop/apps/bridge-dapp#run-local-webb-relayer-and-local-network-alongside-hubble-bridge)
- [Local Standalone Tangle Network\*](https://github.com/webb-tools/tangle/tree/main/scripts#run-a-standalone-tangle-network)

* Note: The Tangle Network is only Required for the EVM Local <> Tangle configuration and is not
  required for the EVM Blanknet configuration.

After that, you can run the relayer:

```bash
cargo run --bin webb-relayer -- -vvv --tmp --config config/development/<config>
```

## Production Configuration

This Repository does not contain any production ready configuration, but they are all examples and
for demonstration purposes only.

However, you can take a look at the following configurations:

1. [exclusive-startegies](../exclusive-strategies/) - These are a set of configurations that are
   used for the exclusive strategies of different roles that you can use the relayer for.
2. [example](../example/) - This is a very simple and minimal configuration for starting a Private
   Transaction Relayer over the Gorli Testnet.
