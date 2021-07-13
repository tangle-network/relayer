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
$ curl -fsSL https://git.io/get-webb-relayer.sh | sh
```

#### Windows

```
$ iwr https://git.io/get-webb-relayer.ps1 -useb | iex
```




## Running and Configuring 🚀

Webb Relayer is easy to run and with flexible config 👌, to test it out first you have to create a config file

Example:

* Create a `config.toml` file and add the following configuration

> By default `webb-relayer` looks for the config at ~/.config/webb-relayer/config.toml on Unix.

```toml
# Webb Relayer configuration

# WebSocket Server Port number
#
# default to 9955
port = 9955

# Interprets the suri string in order to generate a key Pair.
#
# - If `suri` is a possibly `0x` prefixed 64-digit hex string, then it will
#   be interpreted
# directly as a `MiniSecretKey` (aka "seed" in `subkey`).
# - If `suri` is a valid BIP-39 key phrase of 12, 15, 18, 21 or 24 words,
#   then the key will
# be derived from it. In this case:
#   - the phrase may be followed by one or more items delimited by `/`
#     characters.
#   - the path may be followed by `///`, in which case everything after
#     the `#` is treated
# as a password.
# - If `suri` begins with a `/` character it is prefixed with the Substrate
#   public `DEV_PHRASE` and
# interpreted as above.
#
# In this case they are interpreted as HDKD junctions; purely numeric
# items are interpreted as integers, non-numeric items as strings.
# Junctions prefixed with `/` are interpreted as soft junctions, and
# with `//` as hard junctions.
#
# There is no correspondence mapping between SURI strings and the keys
# they represent. Two different non-identical strings can actually
# lead to the same secret being derived. Notably, integer junction
# indices may be legally prefixed with arbitrary number of zeros.
# Similarly an empty password (ending the SURI with `///`) is perfectly
# valid and will generally be equivalent to no password at all.
[substrate.webb]
suri = "//Alice" # For testing purposes.

# Configuration for EVM Networks.
[evm.ganache]
private-key = "0x000000000000000000000000000000000000000000000000000000000000dead"
withdrew-fee = "0x05"
withdrew-gaslimit = "0x350000"

[evm.beresheet]
private-key = "0x000000000000000000000000000000000000000000000000000000000000c0de"
withdrew-fee = "0x05"
withdrew-gaslimit = "0x350000"

# .. and so on.
# see the current supported networks in the next section.
```

Then Simply run

```
$ webb-relayer -vvv -c config.toml # or config.json
```

> Hot Tip 🌶️: you could also use the json format for the config file if you prefer that, and it would work!

* Using Environment Variables:

You could override the values in the configuration file using environment variables so that config value could be prefixed with `WEBB_`

For example, use `WEBB_PORT` to override the port number, and `WEBB_SUBSTRATE_WEBB_SURI` to override the relayer account controller for Webb Substrate Based Network.

That's very useful, you could create an empty config file, with empty values so that you can use env for security reasons!

### Current Supported Networks

1. Substrate Based Networks
    a. Webb (substrate.webb)
    b. Beresheet (substrate.beresheet)
2. EVM Based Networks
    a. Ganache (evm.ganache)
    b. Beresheet (evm.beresheet)
    c. Harmony (evm.harmony)

> Note: as of the current time of writing this we don't support any substrate based relaying transactions
we plan to add them back soon, once we add them to the dApp.

## Using Docker 🐳

To Use Docker in and to run the relayer on any cloud provider, all you need is to create your configuration file
as it is shown above, save it into a `config` directory then you run the following command:

```sh
$ docker run --rm -v "./config:/config" ghcr.io/webb-tools/relayer:v0.1.0-beta.2 # change the version to the latest one.
```

> Note: to use the latest and pre-released version deployed from `main` branch use `edge` as a version.

This will mount a configuration file at the `/config` directory inside the container so it would allow it to read
the configuration you added.

> Note that the `json` configuration format will not work inside the docker.


## Safety ⚡

This crate uses `#![deny(unsafe_code)]` to ensure everything implemented in
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

