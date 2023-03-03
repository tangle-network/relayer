// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;
use tokio::sync::Mutex;
use webb::substrate::dkg_runtime::api::system;
use webb::substrate::subxt::{self, Config, OnlineClient, PolkadotConfig};
use webb_relayer_config::event_watcher::EventsWatcherConfig;
use webb_relayer_context::RelayerContext;
use webb_relayer_store::sled::SledStore;
use webb_relayer_utils::metric;

use crate::substrate::EventHandler;
use crate::SubstrateEventWatcher;



use ark_ff::{BigInteger, PrimeField};
use ark_std::collections::BTreeMap;
use arkworks_native_gadgets::{
	merkle_tree::SparseMerkleTree,
	poseidon::Poseidon,
};
use arkworks_utils::{
	 bytes_vec_to_f, parse_vec
};
use arkworks_setups::{common::setup_params, Curve};
use ark_bn254::Fr as Bn254Fr;

#[derive(Debug, Clone, Default)]
struct TestEventsWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher<PolkadotConfig> for TestEventsWatcher {
    const TAG: &'static str = "Test Event Watcher";

    const PALLET_NAME: &'static str = "System";

    type Client = OnlineClient<PolkadotConfig>;

    type Store = SledStore;
}

#[derive(Debug, Clone, Default)]
struct RemarkedEventHandler;

#[async_trait::async_trait]
impl<PolkadotConfig: Sync + Send + Config> EventHandler<PolkadotConfig>
    for RemarkedEventHandler
{
    type Client = OnlineClient<PolkadotConfig>;
    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: subxt::events::Events<PolkadotConfig>,
    ) -> webb_relayer_utils::Result<bool> {
        let has_event = events.has::<system::events::Remarked>()?;
        Ok(has_event)
    }
    async fn handle_events(
        &self,
        _store: Arc<Self::Store>,
        _client: Arc<Self::Client>,
        (events, block_number): (subxt::events::Events<PolkadotConfig>, u64),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        // find the `Remarked` event(s) in the events
        let remarked_events = events
            .find::<system::events::Remarked>()
            .flatten()
            .collect::<Vec<_>>();
        tracing::debug!(
            "Received `Remarked` Event: {:?} at block number: #{}",
            remarked_events,
            block_number
        );
        Ok(())
    }
}

#[tokio::test]
#[tracing_test::traced_test]
#[ignore = "need to be run manually"]
async fn substrate_event_watcher_should_work() -> webb_relayer_utils::Result<()>
{
    let chain_id = 5u32;
    let store = SledStore::temporary()?;
    let client = OnlineClient::<PolkadotConfig>::new().await?;
    let watcher = TestEventsWatcher::default();
    let config = webb_relayer_config::WebbRelayerConfig::default();
    let ctx = RelayerContext::new(config, store.clone());
    let metrics = ctx.metrics.clone();
    let event_watcher_config = EventsWatcherConfig::default();
    watcher
        .run(
            chain_id,
            client.into(),
            Arc::new(store),
            event_watcher_config,
            vec![Box::<RemarkedEventHandler>::default()],
            metrics,
        )
        .await?;
    Ok(())
}

// merkle tree test
#[test]
fn test_merkle_root(){
    
    let relayer_leaves = vec!["0x282f29f7d6a245199899bd85611257366db8529b1b7454d43a9d43437d0cd53e","0x0c11014a38b133d92fcaddfb4f9f68a9361d63ec447a25c3d473ec601f38e8cc","0x0182b706fc4c79e9efed511e4e11411f758f371b9e571b2bbf5a42749166b45c","0x01e1610c7a62efd09a09f1ba03174cadecbb200068b7a3c41002d79d5078182f","0x20698334a6b3d597293d64c7b7a699f54e8af78f620a15ea0913cb5e41afaef8","0x119d70a942a1577d1772b5e7e9f3709cb0957094f7bdbdc6b096e4fb1d7bcde7","0x016946cbc0bd48edcc59b177c11c1d31721b88cf733d920391de523731a4724b","0x2422ce5958d9b09ed3af9d46eaddf65d423e5633fd6bc0b28c8780193f24d8df","0x011845f64141624571b58cef539a0030c7589e54e6b80c5a6cef9e1d3de38dcf","0x10e9356303ebf1d3ebae932abfeb742bd5ec95aed2be6f92c3e841373d300bb0","0x06f28754e7273b8a0db0774e95487c16617630151cab05642c3d8746abd2a23b","0x1749553630bbb9f20751798ae13bda9144cc7f81bd445b664c9f86e4aa1dc64c","0x23748417d24192ee9c84d7be46b668bd7f70b733f4d24f68f5a38df9251e4367","0x2e70058cb013c2bb3ef9ee04f565bd6844ebfe3b02dd222c92538a58e2b6c63a","0x0f014086c5c9f58a3bc8fcaff247d102e94522f1f7e5bfd027ada423ececfea8","0x2418034774007d76258ae585ced80229caf3ef479d08f0cef3b508a8c5ea9c8a","0x10e8ca0560224909830b9cbc2431dc34aff6fa7261591e526ff9eb2c34e415bb","0x276c6429497479d934b81145a8852a400ef089e96c484d6c38bc795223c8a2f7","0x0a5dfd3c2f54d060842a19cfea627a1fb549791ee8cae90dcab4d30891e2e7b9","0x02b5ee06c2b6cdaa632769facc2fae35c71ad2671a0353f8f940f17517f9df5c","0x06e73e3fe2d5a7db806d3a8e12646aaedbdf8eab8d9490660ae7026e98fd3c87","0x14fb2ff59384a89424426d26930ef1c577a726cf89e5e6942eabcc3c5dd276c6","0x01c6f617fe33fca3d5a05fd258e375917b63e62c678647102898cbfc9b674854","0x0ffd11ab5119c97ef1edd73732c441af0e02516bf358bb5516a433dbda147433","0x02f8e3a1ac237d461310f4630d6764c5818e5c2e56deb570f5321521f7f1b288","0x2979af93b27d457f74a63f8223d8ee08d14900203395a3b0f361f73eb3548227","0x0e867a1be10c187e4e4ce5633e7ac47a9d8779ad986b626c40e372eedb978fca","0x2da7db611de755cc93843b7604639e21411551e942ff3db93d513fe7937ca257","0x06d171e49fcaf9da5d638516a847c2fd47d801a62d571404ad831350850c03b8","0x17c0b0caae5a81d41ae9cb1eafec7d36441cf3a5b8a823c5b2a46a15667054d8","0x174f38725cd1ca2e0ec933c06764665d39b69a5d05a566d6ec8a95232c7aa18a","0x1ff96bf23e2334b88931a147f8399b349201833a6e16caf561f81aca05e966da","0x1fa9d2ecce2ef3e6d39cf3c793a3b7b68a243834439bcb7b000feb83810672d5","0x17075576bf0fd5434785c542a53bf19ab61ee20bc5414ebf51a348276ebda551","0x28acf39e35e0156806e223906357aa466fe1ac844d5b6744099fca1c6bf08612","0x24cd8dfd0d77a27213948bc1f917ff23a20a8b54fe7581034d8d713aee955dda","0x1d457de264943f257a1cb7a6b1848d95f79a202b2cd0051195e160d1fe6348ae","0x0964fbc64d8c336ace1de4512e214bc5e8589555f81901594f1db4576c318bc0","0x1be3ac3ea38efb8eb1089a59242906124bd65ad5ccaf24095a0c21974d82aca5","0x230e50d2e862c3c7dad29cfe4a56687febbfff52c4ab4ed15065e0881219ea37","0x196c7833267ba79d88058be2e41fb69f1bf3015d74d7471ef338fb0ac367c8bc","0x1b6701d9be4689752a45ba9c2ce1bc9e77d71bb2f5063d781f4921f1f3b40b83","0x0cb2fcd53ddd5dd98746e50d699292c7d507e38e66450b30d13627f6044abd25","0x0bdd13892928485e9a97d62aaf2b1250e22fe70ee59a3936951846736b51e06c","0x1372dadcafc2f6a2668656889e20c9cb79f31ed1a32f7ce2edb0b8d922d9d7da","0x0707d973cab6864f6cf0b7df176deb6e0a7160979215a3ae491ca5e4a2fade12","0x262127382ec352d77e6ddc1fb339860f159d62fa5881ea80ad676f95670b1521","0x1384fcf99e2e487aa557eb1e1c4454949d0b6e040a15640b32d2c2952b840536","0x1f780f0350858ff510b267a5f4376aa757af429e50ed5d6e201e345c48c52dd2","0x206199ea0898d0592ae948afcec431fbb8e9dfa97d066b0e13586a527d6db923","0x1c4d5fcddb5bcf2c4f953ceba142323892b9d863c621eccf9c0c41ccfa146926","0x0b9cc8fdcd23caa3b3e45663666c70975bff50b65362aa22b36a90f78113d34f","0x151da8200884ce0a119c6e850b139373b8a24226bda117d8f099c5f2e87edcd8","0x22cbfaeb6fbd05a762464107c47fea791f4247d2bc7fd8597667a80e5e42634b","0x06e77ca8deaf39306c6b3f2551b6d7177aa8ec4ebd5df53421e3456721e79632","0x2aa8ecf3c3b108834f8b17018d63509208d7e6a89835d4aba4d2db2edce0b3cc","0x0eb607eb332427ba1833eda17c93d1be4931eab90c1a696ffd900aed5d57e7bd","0x23225e3eccd1f23e52b54bd050ad3a4dc4048e66ccbc54d415f92bca92830952","0x0100d068b8094d6bbc7c3ffdb6a4efe4a0c63d7d651130be0035a2c31b8bed43","0x0bd1d792b371581fbb20de9a1ae39d28aabf5dc8d27c6dc34404891af8b95d5e"];        let params = setup_params::<Bn254Fr>(Curve::Bn254, 5, 3);
    let poseidon = Poseidon::<Bn254Fr>::new(params);
    let leaves: Vec<Bn254Fr> =
    bytes_vec_to_f(&parse_vec(relayer_leaves).unwrap());
    let pairs: BTreeMap<u32, Bn254Fr> = leaves
        .iter()
        .enumerate()
        .map(|(i, l)| (i as u32, *l))
        .collect();
    type SMT = SparseMerkleTree<Bn254Fr, Poseidon<Bn254Fr>, 30>;
    let default_leaf = Bn254Fr::from(0u64).into_repr().to_bytes_be();
    let smt = SMT::new(&pairs, &poseidon, &default_leaf).unwrap();
    let root = smt.root().into_repr().to_bytes_be();
    let hex_root = hex::encode(root);
    let expected_root = "2f68ec1c3392ddb2cc30554944a4a0ea4ebc76a387cdf7d25ffac9bc05600833";
    assert_eq!(hex_root,expected_root);
}