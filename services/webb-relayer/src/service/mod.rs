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

//! # Relayer Service Module 🕸️
//!
//! A module for starting long-running tasks for event watching.
//!
//! ## Overview
//!
//! Services are tasks which the relayer constantly runs throughout its lifetime.
//! Services handle keeping up to date with the configured chains.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use webb::substrate::subxt;
use webb_proposal_signing_backends::{
    DkgProposalSigningBackend, MockedProposalSigningBackend,
};
use webb_relayer_config::anchor::LinkedAnchorConfig;

use webb_relayer_config::signing_backend::ProposalSigningBackendConfig;
use webb_relayer_context::RelayerContext;
use webb_relayer_handlers::routes::info::handle_relayer_info;
use webb_relayer_handlers::{handle_socket_info, websocket_handler};
use webb_relayer_store::SledStore;

/// EVM Specific Services
pub mod evm;
/// Substrate Specific Services
pub mod substrate;

/// Type alias for [Sled](https://sled.rs)-based database store
pub type Store = SledStore;

/// Sets up the web socket server for the relayer, routing (endpoint queries / requests mapped to
/// handled code) and instantiates the database store. Allows clients to interact with the relayer.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration and database
pub async fn build_web_services(ctx: RelayerContext) -> crate::Result<()> {
    let socket_addr = SocketAddr::new([0, 0, 0, 0].into(), ctx.config.port);
    let api = Router::new()
        .route("/ip", get(handle_socket_info))
        .route("/info", get(handle_relayer_info))
        .merge(evm::build_web_services())
        .merge(substrate::build_web_services());

    let app = Router::new()
        .nest("/api/v1", api)
        .route("/ws", get(websocket_handler))
        .layer(CorsLayer::new().allow_origin(Any))
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(ctx))
        .into_make_service_with_connect_info::<SocketAddr>();

    tracing::info!("Starting the server on {}", socket_addr);
    axum::Server::bind(&socket_addr).serve(app).await?;
    Ok(())
}

/// Starts all background services for all chains configured in the config file.
///
/// Returns a future that resolves when all services are started successfully.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `store` -[Sled](https://sled.rs)-based database store
pub async fn ignite(
    ctx: RelayerContext,
    store: Arc<Store>,
) -> crate::Result<()> {
    tracing::trace!(
        "Relayer configuration: {}",
        serde_json::to_string_pretty(&ctx.config)?
    );
    evm::ignite(&ctx, store.clone()).await?;
    substrate::ignite(ctx.clone(), store.clone()).await?;
    Ok(())
}

/// Proposal signing backend config
#[allow(clippy::large_enum_variant)]
pub enum ProposalSigningBackendSelector {
    /// None
    None,
    /// Mocked
    Mocked(MockedProposalSigningBackend<SledStore>),
    /// Dkg
    Dkg(DkgProposalSigningBackend),
}
/// utility to configure proposal signing backend
pub async fn make_proposal_signing_backend(
    ctx: &RelayerContext,
    store: Arc<Store>,
    typed_chain_id: webb_proposals::TypedChainId,
    linked_anchors: Option<Vec<LinkedAnchorConfig>>,
    proposal_signing_backend: Option<ProposalSigningBackendConfig>,
) -> crate::Result<ProposalSigningBackendSelector> {
    // Check if contract is configured with governance support for the relayer.
    if !ctx.config.features.governance_relay {
        tracing::warn!("Governance relaying is not enabled for relayer");
        return Ok(ProposalSigningBackendSelector::None);
    }

    // we need to check/match on the proposal signing backend configured for this anchor.
    match proposal_signing_backend {
        Some(ProposalSigningBackendConfig::DkgNode(c)) => {
            // if it is the dkg backend, we will need to connect to that node first,
            // and then use the DkgProposalSigningBackend to sign the proposal.
            let dkg_client = ctx
                .substrate_provider::<subxt::PolkadotConfig, _>(c.chain_id)
                .await?;
            let backend = DkgProposalSigningBackend::builder()
                .client(dkg_client)
                .src_chain_id(typed_chain_id)
                .store(store.clone())
                .build();
            Ok(ProposalSigningBackendSelector::Dkg(backend))
        }
        Some(ProposalSigningBackendConfig::Mocked(mocked)) => {
            // if it is the mocked backend, we will use the MockedProposalSigningBackend to sign the proposal.
            // which is a bit simpler than the DkgProposalSigningBackend.
            // get only the linked chains to that anchor.
            let mut signature_bridges: HashSet<webb_proposals::ResourceId> =
                HashSet::new();

            // Check if linked anchors are provided.
            let linked_anchors = match linked_anchors {
                Some(anchors) => {
                    if anchors.is_empty() {
                        tracing::warn!("Misconfigured Network: Linked anchors cannot be empty for governance relaying");
                        return Ok(ProposalSigningBackendSelector::None);
                    } else {
                        anchors
                    }
                }
                None => {
                    tracing::warn!("Misconfigured Network: Linked anchors must be configured for governance relaying");
                    return Ok(ProposalSigningBackendSelector::None);
                }
            };
            linked_anchors.iter().for_each(|anchor| {
                // using chain_id to ensure that we have only one signature bridge
                let resource_id = match anchor {
                    LinkedAnchorConfig::Raw(target) => {
                        let bytes: [u8; 32] = target.resource_id.into();
                        webb_proposals::ResourceId::from(bytes)
                    }
                    _ => unreachable!("unsupported"),
                };
                signature_bridges.insert(resource_id);
            });
            let backend = MockedProposalSigningBackend::builder()
                .store(store.clone())
                .private_key(mocked.private_key)
                .signature_bridges(signature_bridges)
                .build();
            Ok(ProposalSigningBackendSelector::Mocked(backend))
        }
        None => {
            tracing::warn!("Misconfigured Network: Proposal signing backend must be configured for governance relaying");
            Ok(ProposalSigningBackendSelector::None)
        }
    }
}
