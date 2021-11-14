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

* Create a .env file with the following values for the networks you wish to support

```
WEBB_EVM_<network>_ENABLED=true
WEBB_EVM_<network>_PRIVATE_KEY=<0X_PREFIXED_PRIVATE_KEY>

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

