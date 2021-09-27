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
# Webb Relayer Network Port
# default: 9955
port = 9955
[substrate.webb]
suri = "//Alice"

[evm.harmony]
# Http(s) Endpoint for quick Req/Res
http-endpoint = "https://api.s1.b.hmny.io"
# Websocket Endpoint for long living connections
ws-endpoint = "wss://ws.s1.b.hmny.io"
# chain specific id.
chain-id = 1666700001
# The Private Key of this account on this network
# the format is more dynamic here:
# 1. if it starts with '0x' then this would be raw (64 bytes) hex encoded
#    private key.
#    Example: 0x8917174396171783496173419137618235192359106130478137647163400318
#
# 2. if it starts with '$' then it would be considered as an Enviroment variable
#    of a hex-encoded private key.
#    Example: $HARMONY_PRIVATE_KEY
#
# 3. if it starts with '> ' then it would be considered as a command that
#    the relayer would execute and the output of this command would be the
#    hex encoded private key.
#    Example: > pass harmony-privatekey
#
# 4. if it doesn't contains special characters and has 12 or 24 words in it
#    then we should process it as a mnemonic string: 'word two three four ...'
private-key = "0x8917174396171783496173419137618235192359106130478137647163400318"

# chain contracts
[[evm.harmony.contracts]]
# The contract can be one of these values
# - Anchor (tornado protocol)
# - Anchor2 (darkwebb protocol)
# - Bridge
# - GovernanceBravoDelegate
contract = "Anchor"
# The address of this contract on this chain.
address = "0x4c37863bf2642Ba4e8De7e746500C700540119E8"
# the block number where this contract got deployed at.
deployed-at = 13600000
# The size of this contract
# Note: only available for `Anchor` and `Anchor2` contracts.
# and would error otherwise.
size = 0.0000000001
# control the events watcher for this contract
# Note: only available for `Anchor` and `Anchor2` contracts.
events-watcher = { enabled = false, polling-interval = 3000 }
# The fee percentage that your account will receive when you relay a transaction
# over this chain.
withdraw-fee-percentage = 0.05
# A hex value of the gaslimit when doing a withdraw relay transaction
# on this chain.
withdraw-gaslimit = "0x350000"

[[evm.harmony.contracts]]
contract = "Anchor"
address = "0x7cd1F52e5EEdf753e99D945276a725CE533AaD1a"
deployed-at = 12040000
size = 100
events-watcher = { enabled = false, polling-interval = 3000 }
withdraw-fee-percentage = 0.05
withdraw-gaslimit = "0x350000"

# and so on for other networks and other contracts ...


```
> Checkout [config.toml](./config.toml) as an example.

Then Simply run

```
webb-relayer -vvv -c config.toml # or config.json
```

> Hot Tip 🌶️: you could also use the json format for the config file if you prefer that, and it would work!

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

