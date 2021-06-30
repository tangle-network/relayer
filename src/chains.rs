pub mod substrate {
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
    pub trait EvmChain {
        fn name() -> String;
        fn endpoint() -> &'static str;
        fn chain_id() -> u32;
        fn contracts() -> HashMap<&'static str, u128>;
    }

    macro_rules! define_chain {
        ($name:ident => {
            endpoint: $endpoint:expr,
            chain_id: $chain_id:expr,
            contracts: [
                $({
                    size: $size:expr,
                    address: $address:expr,
                }),*
            ],
        }) => {
            #[derive(Debug, Clone, Copy, Eq, PartialEq)]
            pub struct $name;
            impl EvmChain for $name {
                fn name() -> String { stringify!($name).to_lowercase() }

                fn endpoint() -> &'static str { $endpoint }

                fn chain_id() -> u32 { $chain_id }

                fn contracts() -> HashMap<&'static str, u128> {
                    let mut map = HashMap::new();
                    $(
                        map.insert($address, $size);
                    )*
                    map
                }
            }
       };
    }

    define_chain! {
        Edgeware => {
            endpoint: "https://mainnet1.edgewa.re/evm",
            chain_id: 2021,
            contracts: [],
        }
    }
    define_chain! {
        Beresheet => {
            endpoint: "http://beresheet1.edgewa.re:9933",
            chain_id: 2022,
            contracts: [
                {
                    size: 10,
                    address: "0x5f771fc87F87DB48C9fB11aA228D833226580689",
                },
                {
                    size: 100,
                    address: "0x2ee2e51cab1561E4482cacc8Be8b46CE61E46991",
                },
                {
                    size: 1000,
                    address: "0x5696b4AfBc169454d7FA26e0a41828d445CFae20",
                },
                {
                    size: 10000,
                    address: "0x626FEc5Ffa7Bf1EE8CEd7daBdE545630473E3ABb",
                }
            ],
        }
    }
    define_chain! {
        Harmoney => {
            endpoint: "",
            chain_id: 0,
            contracts: [],
        }
    }
}
