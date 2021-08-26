pub mod substrate {
    #![allow(dead_code)]
    macro_rules! define_chain {
        ($name:ident => $endpoint:expr) => {
            #[derive(Debug, Clone, Copy, Eq, PartialEq)]
            pub struct $name;
            impl $name {
                pub const fn endpoint() -> &'static str { $endpoint }
            }
        };
        ($($name:ident => $endpoint:expr),+) => {
            $(define_chain!($name => $endpoint);)+
        }
    }

    define_chain! {
        Edgeware => "wss://mainnet1.edgewa.re",
        Beresheet => "wss://beresheet1.edgewa.re",
        Webb => "ws://127.0.0.1:9944"
    }
}

pub mod evm {
    use std::collections::HashMap;
    use std::str::FromStr;

    use webb::evm::ethereum_types::Address;

    #[derive(Debug, Clone, Copy)]
    pub struct DeployedContract {
        pub address: Address,
        pub size: f64,
        pub deplyed_at: u64,
    }
    /// All Supported Chains by Webb Realyer.
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum ChainName {
        Edgeware,
        Webb,
        Ganache,
        Ganache1,
        Ganache2,
        Beresheet,
        Harmony,
        Rinkeby,
    }

    pub trait EvmChain {
        fn name() -> ChainName;
        fn endpoint() -> &'static str;
        fn ws_endpoint() -> &'static str;
        fn polling_interval_ms() -> u64;
        fn chain_id() -> u32;
        fn anchor_contracts() -> HashMap<Address, DeployedContract>;
    }

    macro_rules! define_chain {
        ($name:ident => {
            endpoint: $endpoint:expr,
            ws_endpoint: $ws_endpoint:expr,
            polling_interval_ms: $polling_interval_ms:expr,
            chain_id: $chain_id:expr,
            anchor_contracts: [
                $({
                    size: $size:expr,
                    address: $address:expr,
                    at: $at:expr,
                }),*
            ],
        }) => {
            #[derive(Debug, Clone, Copy, Eq, PartialEq)]
            pub struct $name;
            impl EvmChain for $name {
                fn name() -> ChainName { ChainName::$name }

                fn endpoint() -> &'static str { $endpoint }

                fn ws_endpoint() -> &'static str { $ws_endpoint }

                fn polling_interval_ms() -> u64 { $polling_interval_ms }

                fn chain_id() -> u32 { $chain_id }

                fn anchor_contracts() -> HashMap<Address, DeployedContract> {
                    #[allow(unused_mut)]
                    let mut map = HashMap::new();
                    $(
                        let address = Address::from_str($address).unwrap();
                        let contract = DeployedContract {
                            address,
                            size: $size as f64,
                            deplyed_at: $at,
                        };
                        map.insert(address, contract);
                    )*
                    map
                }
            }
       };
    }

    define_chain! {
        Edgeware => {
            endpoint: "https://mainnet1.edgewa.re/evm",
            ws_endpoint: "wss://mainnet1.edgewa.re/evm",
            polling_interval_ms: 7000,
            chain_id: 2021,
            anchor_contracts: [],
        }
    }

    define_chain! {
        Ganache => {
            endpoint: "http://localhost:8545",
            ws_endpoint: "ws://localhost:8545",
            polling_interval_ms: 1000,
            chain_id: 1337,
            anchor_contracts: [
                {
                    size: 1,
                    address: "0xFBD61C9961e0bf872B5Ec041b718C0B2a106Ce9D",
                    at: 1,
                }
            ],
        }
    }

    define_chain! {
        Ganache1 => {
            endpoint: "http://localhost:8546",
            ws_endpoint: "ws://localhost:8545",
            polling_interval_ms: 1000,
            chain_id: 1337,
            anchor_contracts: [
                {
                    size: 1,
                    address: "0xFBD61C9961e0bf872B5Ec041b718C0B2a106Ce9D",
                    at: 1,
                }
            ],
        }
    }

    define_chain! {
        Ganache2 => {
            endpoint: "http://localhost:8547",
            ws_endpoint: "ws://localhost:8545",
            polling_interval_ms: 1000,
            chain_id: 1337,
            anchor_contracts: [
                {
                    size: 1,
                    address: "0xFBD61C9961e0bf872B5Ec041b718C0B2a106Ce9D",
                    at: 1,
                }
            ],
        }
    }

    define_chain! {
        Beresheet => {
            endpoint: "https://beresheet.edgewa.re/evm",
            ws_endpoint: "wss://beresheet.edgewa.re",
            polling_interval_ms: 7000,
            chain_id: 2022,
            anchor_contracts: [
                {
                    size: 10,
                    address: "0xf0EA8Fa17daCF79434d10C51941D8Fc24515AbE3",
                    at: 299_740,
                },
                {
                    size: 100,
                    address: "0xc0d863EE313636F067dCF89e6ea904AD5f8DEC65",
                    at: 299_740,
                },
                {
                    size: 1_000,
                    address: "0xc7c6152214d0Db4e161Fa67fB62811Be7326834A",
                    at: 299_740,
                },
                {
                    size: 10_000,
                    address: "0xf0290d80880E3c59512e454E303FcD48f431acA3",
                    at: 299_740,
                }
            ],
        }
    }

    define_chain! {
        Harmony => {
            endpoint: "https://api.s1.b.hmny.io",
            ws_endpoint: "wss://ws.s1.b.hmny.io",
            polling_interval_ms: 3000,
            chain_id: 1666700001,
            anchor_contracts: [
                {
                    size: 0.0000000001,
                    address: "0x4c37863bf2642Ba4e8De7e746500C700540119E8",
                    at: 13_600_000,
                },
                {
                    size: 100,
                    address: "0x7cd1F52e5EEdf753e99D945276a725CE533AaD1a",
                    at: 12_040_000,
                },
                {
                    size: 1_000,
                    address: "0xD7f9BB9957100310aD397D2bA31771D939BD4731",
                    at: 12_892_487,
                },
                {
                    size: 10_000,
                    address: "0xeE2eB8F142e48e5D1bDD34e0924Ed3B4aa0d4222",
                    at: 12_892_648,
                },
                {
                    size: 100_000,
                    address: "0x7cd173094eF78FFAeDee4e14576A73a79aA716ac",
                    at: 12_892_840,
                }
            ],
        }
    }

    define_chain! {
        Webb => {
            endpoint: "",
            ws_endpoint: "",
            polling_interval_ms: 0,
            chain_id: 0,
            anchor_contracts: [],
        }
    }

    define_chain! {
        Rinkeby => {
            endpoint: "https://rinkeby.infura.io/v3/9aa3d95b3bc440fa88ea12eaa4456161",
            ws_endpoint: "wss://rinkeby.infura.io/ws/v3/9aa3d95b3bc440fa88ea12eaa4456161",
            polling_interval_ms: 30000,
            chain_id: 4,
            anchor_contracts: [
                {
                    size: 0.1,
                    address: "0x626FEc5Ffa7Bf1EE8CEd7daBdE545630473E3ABb",
                    at: 8_896_800,
                },
                {
                    size: 1,
                    address: "0x979cBd4917e81447983ef87591B9E1ab21727a61",
                    at: 8_896_800,
                }
            ],
        }
    }
}
