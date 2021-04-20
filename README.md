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




## Running and Configuring

Webb Relayer is easy to run and config, to test it out first you have to create a config file

Example:

* Create a `config.toml` file and add the following configuration

> By default `webb-relayer` looks for the config at ~/.config/webb-relayer/config.toml

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

suri = "//Alice" # For testing purposes.
```

Then Simply run

```
$ webb-relayer -vvv -c config.toml
```

* Using Environment Variables:

You could override the values in the configuration file using environment variables
Any config value could be prefixed with `WEBB_` .

For example, use `WEBB_PORT` to override the port number, and `WEBB_SURI` to override the relayer account controller.


## Using Docker

// TODO

## Safety

This crate uses `#![deny(unsafe_code)]` to ensure everything implemented in
100% Safe Rust.

## Contributing

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

