use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use ethers::abi::Tokenizable;
use ethers::prelude::*;
use ethers::utils::{Ganache, GanacheInstance};
use webb::evm::contract::anchor::AnchorContract;
use webb::evm::ethers;
use webb::evm::note::{Deposit, Note};

pub fn create_contract_factory<P: Into<PathBuf>, M: Middleware>(
    path: P,
    client: Arc<M>,
) -> anyhow::Result<ContractFactory<M>> {
    let json_file = fs::read_to_string(path.into())
        .context("reading contract json file")?;
    let raw: serde_json::Value =
        serde_json::from_str(&json_file).context("parsing json file")?;
    let abi = serde_json::from_value(raw["abi"].clone())?;
    let bytecode_hex = raw["bytecode"].as_str().expect("bytecode");
    let bytecode = hex::decode(&bytecode_hex[2..])
        .context("decoding bytecode from hex to bytes")?;
    Ok(ContractFactory::new(abi, bytecode.into(), client))
}

pub async fn deploy_anchor_contract<M: Middleware + 'static>(
    client: Arc<M>,
) -> anyhow::Result<Address> {
    let hasher_factory = create_contract_factory(
        "tests/build/contracts/Hasher.json",
        client.clone(),
    )
    .context("create hasher factory")?;
    let verifier_factory = create_contract_factory(
        "tests/build/contracts/Verifier.json",
        client.clone(),
    )
    .context("create verifier factory")?;
    let native_anchor_factory = create_contract_factory(
        "tests/build/contracts/NativeAnchor.json",
        client.clone(),
    )
    .context("create native anchor factory")?;
    let hasher_instance = hasher_factory
        .deploy(())
        .context("deploy hasher")?
        .send()
        .await?;
    let verifier_instance = verifier_factory
        .deploy(())
        .context("deploy verifier")?
        .send()
        .await?;

    let verifier_address = verifier_instance.address().into_token();
    let hasher_address = hasher_instance.address().into_token();
    let denomination = ethers::utils::parse_ether("1")?.into_token();
    let merkle_tree_hight = 20u32.into_token();
    let args = (
        verifier_address,
        hasher_address,
        denomination,
        merkle_tree_hight,
    );
    let native_anchor_instance = native_anchor_factory
        .deploy(args)
        .context("deploy native anchor")?
        .send()
        .await?;
    Ok(native_anchor_instance.address())
}

pub async fn launch_ganache() -> GanacheInstance {
    tokio::task::spawn_blocking(|| Ganache::new().port(1998u16).spawn())
        .await
        .unwrap()
}

pub async fn make_deposit<M: 'static + Middleware>(
    rng: &mut impl rand::Rng,
    contract: &AnchorContract<M>,
    leaves: &mut Vec<H256>,
) -> anyhow::Result<()> {
    let note = Note::builder()
        .with_chain_id(1337u64)
        .with_amount(1u64)
        .with_currency("ETH")
        .build(rng);
    let deposit: Deposit = note.clone().into();
    let tx = contract
        .deposit(deposit.commitment.into())
        .value(ethers::utils::parse_ether(note.amount)?);
    let result = tx.send().await?;
    let _receipt = result.await?;
    leaves.push(deposit.commitment);
    Ok(())
}
