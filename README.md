<h1 align="center">Webb Relayer</h1>
<div align="center">
  <a href="https://www.webb.tools/">
    <img alt="Webb Logo" src="./assets/webb-icon.svg" width="15%" height="30%" />
  </a>
</div>
<p align="center">
    <strong>🕸️  The Webb Relayer  🧑‍✈️</strong>
    <br />
    <sub> ⚠️ Beta Software ⚠️ </sub>
</p>

<br />

## Install ⛹️

#### Unix (Linux, macOS, WSL2, ..)

```
git clone https://github.com/webb-tools/relayer.git

cargo build --release
```

## Local Substrate Mixer
Ideal for development, Run a relayer for a local substrate node that integrates our pallets:
[webb-standalone-node](https://github.com/webb-tools/protocol-substrate/)

Run the relayer with our preset configuration like so: 
```
./target/release/webb-relayer -c config/local-substrate -vvvv
```

## Running and Configuring 🚀

Webb Relayer is easy to run and with flexible config 👌, to test it out first you have to create a config file

Example:

* Create a .env file with the following values for the networks you wish to support

```
WEBB_EVM_<network>_ENABLED=true
WEBB_EVM_<network>_PRIVATE_KEY=<0X_PREFIXED_PRIVATE_KEY>

WEBB_EVM_<network>_BENEFICIARY=<0X_PREFIXED_ADDRESS>

```

> Checkout [config](./config) for useful default configurations for many networks. These config files can be changed to your preferences, and are enabled with the .env configuration listed above.

Then Simply run

```
webb-relayer -vvv -c ./config
```

> Hot Tip 🌶️: you could also use the json format for the config files if you prefer that, and it would work!

## Using Docker 🐳

To Use Docker in and to run the relayer on any cloud provider, all you need is to create your configuration and .env files
as it is shown above, save it into a `config` directory then you run the following command:

```sh
docker run --rm -v "<ABSOLUTE_PATH_TO_CONFIGS_DIRECTORY>:/config" --env-file .env -p 9955:9955 ghcr.io/webb-tools/relayer:edge
```

> Note: this uses the latest and pre-released version deployed from `main` branch, change `edge` to the latest stable release version.

This will mount a configuration files at the `/config` directory inside the container so it would allow it to read the configuration you added.

## Overview

    - src/
        - events_watcher/: Sync to different network types (EVM, Substrate), and act on different events. 
        - store/: Logic for storing information with different backends.
        - config.rs: Functionality related to parsing of configurable values.
        - context.rs: Access the parsed configuration and generate providers and wallets.
        - handler.rs: Logic for what to do when a client is interacting with this relayer.
        - main.rs: Build and start the relayer.
        - service.rs: The entry for tasks once the relayer is operating.
        - tx_queue.rs: A transaction queue for orderly handling of transactions.

## Safety ⚡

This crate uses `#![deny(unsafe_code)]` to ensure everything is implemented in
100% Safe Rust.

## Contributing 🧑‍🤝‍🧑

Want to join us? take a look at some of these issues:

- [Issues labeled "good first issue"][good-first-issue]
- [Issues labeled "help wanted"][help-wanted]

[good-first-issue]: https://github.com/webb-tools/relayer/labels/good%20first%20issue
[help-wanted]: https://github.com/webb-tools/relayer/labels/help%20wanted

## License

<sup>
Licensed under <a href="LICENSE">Apache 2.0 license</a>.
</sup>

<br/>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache 2.0 license, shall
be licensed as above, without any additional terms or conditions.
</sub>

