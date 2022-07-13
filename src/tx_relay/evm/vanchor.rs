use ethereum_types::U256;
use std::{collections::HashMap, sync::Arc};
use webb::evm::{
    contract::protocol_solidity::{
        variable_anchor::{ExtData, Proof},
        VAnchorContract,
    },
    ethers::prelude::{Signer, SignerMiddleware},
};

use crate::config::AnchorWithdrawConfig;
use crate::{
    context::RelayerContext,
    handler::{
        calculate_fee, CommandResponse, CommandStream, EvmCommand,
        NetworkStatus, WithdrawStatus,
    },
    tx_relay::evm::handle_evm_tx,
};
/// Handler for VAnchor commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_vanchor_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: EvmCommand,
    stream: CommandStream,
) {
    use CommandResponse::*;
    let cmd = match cmd {
        EvmCommand::VAnchor(cmd) => cmd,
        _ => return,
    };

    let requested_chain = cmd.chain_id;
    let chain = match ctx.config.evm.get(&requested_chain.to_string()) {
        Some(v) => v,
        None => {
            tracing::warn!("Unsupported Chain: {}", requested_chain);
            let _ = stream.send(Network(NetworkStatus::UnsupportedChain)).await;
            return;
        }
    };
    let supported_contracts: HashMap<_, _> = chain
        .contracts
        .iter()
        .cloned()
        .filter_map(|c| match c {
            crate::config::Contract::VAnchor(c) => Some(c),
            _ => None,
        })
        .map(|c| (c.common.address, c))
        .collect();
    // get the contract configuration
    let contract_config = match supported_contracts.get(&cmd.id) {
        Some(config) => config,
        None => {
            tracing::warn!("Unsupported Contract: {:?}", cmd.id);
            let _ = stream
                .send(Network(NetworkStatus::UnsupportedContract))
                .await;
            return;
        }
    };
    // validate contract withdraw configuration
    let withdraw_config: &AnchorWithdrawConfig = match &contract_config
        .withdraw_config
    {
        Some(cfg) => cfg,
        None => {
            tracing::error!("Misconfigured Network : ({}). Please set withdraw configuration.", cmd.chain_id);
            let _ = stream
                .send(Error(format!("Misconfigured Network : ({}). Please set withdraw configuration.", cmd.chain_id)))
                .await;
            return;
        }
    };

    let wallet = match ctx.evm_wallet(&cmd.chain_id.to_string()).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Misconfigured Network: {}", e);
            let _ = stream
                .send(Error(format!(
                    "Misconfigured Network: {:?}",
                    cmd.chain_id
                )))
                .await;
            return;
        }
    };
    // validate the relayer address first before trying
    // send the transaction.
    let reward_address = match chain.beneficiary {
        Some(account) => account,
        None => wallet.address(),
    };

    if cmd.ext_data.relayer != reward_address {
        let _ = stream
            .send(Network(NetworkStatus::InvalidRelayerAddress))
            .await;
        return;
    }

    // validate that the roots are multiple of 32s
    let roots = cmd.proof_data.roots.to_vec();
    if roots.len() % 32 != 0 {
        let _ = stream
            .send(Withdraw(WithdrawStatus::InvalidMerkleRoots))
            .await;
        return;
    }

    tracing::debug!(
        "Connecting to chain {:?} .. at {}",
        cmd.chain_id,
        chain.http_endpoint
    );
    let _ = stream.send(Network(NetworkStatus::Connecting)).await;
    let provider = match ctx.evm_provider(&cmd.chain_id.to_string()).await {
        Ok(value) => {
            let _ = stream.send(Network(NetworkStatus::Connected)).await;
            value
        }
        Err(e) => {
            let reason = e.to_string();
            let _ =
                stream.send(Network(NetworkStatus::Failed { reason })).await;
            let _ = stream.send(Network(NetworkStatus::Disconnected)).await;
            return;
        }
    };

    let client = SignerMiddleware::new(provider, wallet);
    let client = Arc::new(client);
    let contract = VAnchorContract::new(cmd.id, client);

    // check the fee
    // TODO: Match this up in the context of variable transfers
    let expected_fee = calculate_fee(
        withdraw_config.withdraw_fee_percentage,
        cmd.ext_data.ext_amount.as_u128().into(),
    );
    let (_, unacceptable_fee) =
        U256::overflowing_sub(cmd.ext_data.fee, expected_fee);
    if unacceptable_fee {
        tracing::error!("Received a fee lower than configuration");
        let msg = format!(
            "User sent a fee that is too low {} but expected {}",
            cmd.ext_data.fee, expected_fee,
        );
        let _ = stream.send(Error(msg)).await;
        return;
    }

    let ext_data = ExtData {
        recipient: cmd.ext_data.recipient,
        relayer: cmd.ext_data.relayer,
        fee: cmd.ext_data.fee,
        ext_amount: cmd.ext_data.ext_amount,
        encrypted_output_1: cmd.ext_data.encrypted_output1,
        encrypted_output_2: cmd.ext_data.encrypted_output2,
    };

    let proof = Proof {
        proof: cmd.proof_data.proof,
        roots: roots.into(),
        ext_data_hash: cmd.proof_data.ext_data_hash.to_fixed_bytes(),
        public_amount: U256::from_little_endian(
            &cmd.proof_data.public_amount.to_fixed_bytes(),
        ),
        input_nullifiers: cmd
            .proof_data
            .input_nullifiers
            .iter()
            .map(|v| v.to_fixed_bytes())
            .collect(),
        output_commitments: [
            cmd.proof_data.output_commitments[0].to_fixed_bytes(),
            cmd.proof_data.output_commitments[1].to_fixed_bytes(),
        ],
    };
    tracing::trace!(?proof, ?ext_data, "Client Proof");
    let call = contract.transact(proof, ext_data);
    tracing::trace!("About to send Tx to {:?} Chain", cmd.chain_id);
    handle_evm_tx(call, stream).await;
}
