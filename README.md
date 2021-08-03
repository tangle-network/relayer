<h1 align="center">Webb Relayer</h1>

<p align="center">
    <strong>🕸️  The Webb Relayer  🧑‍✈️</strong>
    <br />
    <sub> ⚠️ Beta Software ⚠️ </sub>
</p>

<br />

## Install ⛹️

#### Unix (Linux, macOS, WSL2, ..)

```
curl -fsSL https://git.io/get-webb-relayer.sh | sh
```

## Running and Configuring 🚀

Webb Relayer is easy to run and with flexible config 👌, to test it out first you have to create a config file

Example:

* Create a `config.toml` file and add the following configuration

	> On Unix `webb-relayer` looks for the config at ~/.config/webb-relayer/config.toml .

	> On MacOS `webb-relayer` looks for the config at ~/Library/Application Support/tools.webb.webb-relayer/config.toml .

```toml
# Webb Relayer configuration

# WebSocket Server Port number
#
# default to 9955
port = 9955

# Configuration for EVM Networks.

[evm.ganache]
private-key = "0x000000000000000000000000000000000000000000000000000000000000dead"
withdrew-fee-percentage = "0.05"
withdrew-gaslimit = "0x350000"
enable-leaves-watcher = true

[evm.beresheet]
private-key = "0x000000000000000000000000000000000000000000000000000000000000dead"
withdrew-fee-percentage = "0.05"
withdrew-gaslimit = "0x350000"
enable-leaves-watcher = false

[evm.harmony]
private-key = "0x000000000000000000000000000000000000000000000000000000000000dead"
withdrew-fee-percentage = "0.05"
withdrew-gaslimit = "0x350000"
enable-leaves-watcher = true

# .. and so on.
# see the current supported networks in the next section.
```

Then Simply run

```
webb-relayer -vvv -c config.toml # or config.json
```

> Hot Tip 🌶️: you could also use the json format for the config file if you prefer that, and it would work!

* Using Environment Variables:

You could override the values in the configuration file using environment variables so that config value could be prefixed with `WEBB_`

For example, use `WEBB_PORT` to override the port number, and `WEBB_SUBSTRATE_WEBB_SURI` to override the relayer account controller for Webb Substrate Based Network.

That's very useful, you could create an empty config file, with empty values so that you can use env for security reasons!

### Current Supported Networks

1. Substrate Based Networks
    * Webb (substrate.webb)
    * Beresheet (substrate.beresheet)
2. EVM Based Networks
    * Ganache (evm.ganache)
    * Beresheet (evm.beresheet)
    * Harmony (evm.harmony)

> Note: as of the current time of writing this we don't support any substrate based relaying transactions
we plan to add them back soon, once we add them to the dApp.

## Using Docker 🐳

To Use Docker in and to run the relayer on any cloud provider, all you need is to create your configuration file
as it is shown above, save it into a `config` directory then you run the following command:

```sh
docker run --rm -v "./config:/config" -e WEBB_PORT=9955 -p 9955:9955 ghcr.io/webb-tools/relayer:edge
```

> Note: this uses the latest and pre-released version deployed from `main` branch, change `edge` to the latest stable release version.

This will mount a configuration file at the `/config` directory inside the container so it would allow it to read
the configuration you added.

> Note that the `json` configuration format will not work inside the docker.


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
Licensed under <a href="LICENSE">GPLV3 license</a>.
</sup>

<br/>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the GPLV3 license, shall
be licensed as above, without any additional terms or conditions.
</sub>

