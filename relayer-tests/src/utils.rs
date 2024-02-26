use reqwest::Response;
use std::{path::PathBuf, str::FromStr};
use webb::evm::ethers::types::H160;
use webb_evm_test_utils::v_bridge::VAnchorBridgeInfo;
use webb_relayer_handler_utils::EvmVanchorCommand;

pub fn get_git_root_path() -> PathBuf {
    let git_root = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .expect("Failed to get git root")
        .stdout;
    let git_root = std::str::from_utf8(&git_root)
        .expect("Failed to parse git root")
        .trim()
        .to_string();
    PathBuf::from(&git_root)
}

// Returns bridge info for saved chain state after deploying contracts.
pub fn get_hermes_bridge_info() -> VAnchorBridgeInfo {
    VAnchorBridgeInfo {
        bridge: H160::from_str("0x5fbdb2315678afecb367f032d93f642f64180aa3")
            .unwrap(),
        vanchor: H160::from_str("0x68b1d87f95878fe05b998f19b66f4baba5de1aed")
            .unwrap(),
        vanchor_handler: H160::from_str(
            "0xe7f1725e7734ce288f8367e1bb143e90bb3f0512",
        )
        .unwrap(),
        treasury: H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9")
            .unwrap(),
        treasury_handler: H160::from_str(
            "0x9fe46736679d2d9a65f0992f2272de9f3c7fa6e0",
        )
        .unwrap(),
        fungible_token_wrapper: H160::from_str(
            "0x0b306bf915c4d645ff596e518faf3f9669b97016",
        )
        .unwrap(),
        token_wrapper_handler: H160::from_str(
            "0x9a676e781a523b5d0c0e43731313a708cb607508",
        )
        .unwrap(),
    }
}

pub async fn send_evm_tx(
    chain_id: u32,
    vanchor_address: H160,
    evm_cmd: EvmVanchorCommand,
) -> Result<Response, String> {
    let api = format!(
        "http://0.0.0.0:9955/api/v1/send/evm/{}/{}",
        chain_id, vanchor_address
    );
    let payload =
        serde_json::to_string(&evm_cmd).expect("Failed to serialize JSON");
    // send post api
    let response = reqwest::Client::new()
        .post(api)
        .json(&payload)
        .send()
        .await
        .unwrap();
    Ok(response)
}
