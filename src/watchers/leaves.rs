use crate::*;
use crate::chains::evm::*;

pub async fn start_leave_cache_service<P>(
    path: Option<P>,
    ctx: &RelayerContext,
) -> anyhow::Result<leaf_cache::SledLeafCache>
where
    P: AsRef<Path>,
{
    let dirs = ProjectDirs::from(
        crate::PACKAGE_ID[0],
        crate::PACKAGE_ID[1],
        crate::PACKAGE_ID[2],
    )
    .context("failed to get config")?;
    let p = match path.as_ref() {
        Some(p) => p.as_ref().to_path_buf(),
        None => dirs.data_local_dir().to_path_buf(),
    };
    let db_path = match path.zip(p.parent()) {
        Some((_, parent)) => parent.join("leaves"),
        None => p.join("leaves"),
    };

    let store = leaf_cache::SledLeafCache::open(db_path)?;
    // some macro magic to not repeat myself.
    macro_rules! start_network_watcher_for {
        ($chain: ident) => {
            // check to see if we should enable the leaves watcher
            // for this chain.
            let leaf_watcher_enabled = ctx.leaves_watcher_enabled::<chains::evm::$chain>();
            let contracts = chains::evm::$chain::anchor_contracts()
                .into_values()
                .filter(|_| leaf_watcher_enabled) // will skip all if `false`.
                .collect::<Vec<_>>();
            for contract in contracts {
                let watcher = leaf_cache::LeavesWatcher::new(
                    chains::evm::$chain::ws_endpoint(),
                    store.clone(),
                    contract.address,
                    contract.deplyed_at,
                    chains::evm::$chain::polling_interval_ms(),
                );
                let task = async move {
                    tokio::select! {
                        _ = watcher.run() => {
                            tracing::warn!("watcher for {} stopped", stringify!($chain));
                        },
                        _ = tokio::signal::ctrl_c() => {
                            tracing::debug!(
                                "Stopping the leaves watcher for {} ({})",
                                stringify!($chain),
                                contract.address,
                            );
                        }
                    };
                };
                tracing::debug!(
                    "leaves watcher for {} ({}) Started.",
                    stringify!($chain),
                    contract.address,
                );
                tokio::task::spawn(task);
            }
        };
        ($($chain: ident),+) => {
            $(
                start_network_watcher_for!($chain);
            )+
        }
    }

    start_network_watcher_for!(Ganache, Beresheet, Harmony, Rinkeby);
    Ok(store)
}