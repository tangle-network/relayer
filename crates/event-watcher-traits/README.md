## Event Watcher traits

A well-defined set of traits that define how to write event watchers for both EVM-based chains and Substrate chains.

## Features

- ⚡ Very fast, can sync and handle 1000s of blocks per second.
- 🔄 Just a thin wrapper around a loop over blocks and handling the events.
- 🔁 Auto recover, auto retry when it fails to handle events.
- 📊 Shows progress while handling events.
- ✂️ Domain separation between watching for events and handling them.

### EVM Event Watcher

Here are the key components of the EVM Event Watcher traits:

- **EventWatcher**: The main trait that needs to be implemented for an event watcher. It defines the contract, events, and store types to be used and provides a method for starting the event watcher.
- **WatchableContract**: A trait that needs to be implemented by the contract struct being watched. It provides information such as the block at which the contract was deployed, the polling interval for events, and the maximum number of blocks to sync per step.
- **EventHandlers**: Handlers for specific event types. Each event handler is responsible for executing code when a specific event occurs.

The way this event watcher architecture works is as follows:

You have a smart contract that emits some events, and you need to execute some code when an event gets emitted. To achieve this, you implement the EventWatcher trait for your smart contract struct and define a set of EventHandlers, where each handler is specific to one event type. This architecture allows having one event watcher per contract with many event handlers.

#### EVM Event Watcher with Example

Let's take an example. Imagine we want to compute how many USDC tokens got transferred in the last 24 hours, from 00:00 UTC to 23:59 UTC each day. First, we need to have bindings to the USDC contract, and since we only care about the Transfer events.

```rust
ethers::abigen!(
  USDC,
  r#"[
    function balanceOf(address account) external view returns (uint256)
    function decimals() external view returns (uint8)
    function symbol() external view returns (string memory)
    function transfer(address to, uint256 amount) external returns (bool)
    event Transfer(address indexed from, address indexed to, uint256 value)
    ]"#
);
```

To be able to use the `USDC` contract in the `EventWatcher` trait, we will need to implement the `WatchableContract` trait, which is a very simple trait that defines a few things like at which block this contract got deployed at, and the polling interval for events, how much blocks we should sync per-step ..etc.

```rust
use ethers::types::*;

impl WatchableContract for USDC {
  fn deployed_at(&self) -> U64 { U64::from(12740001) }
  fn polling_interval(&self) -> Duration { Duration::from_secs(12) }
  fn max_blocks_per_step(&self) -> U64 { U64::from(1000) }
  fn print_progress_interval(&self) -> Duration { Duration::from_secs(60) }
}
```

We can create our Stateless Event watcher struct, which is just a simple struct that implements `EventWatcher`, which does not require any method implementations, we can use the default implemented method.

Before doing so, we will need to create a client, store, and `RelayerContext`.

```rust
type EthersClient = Provider<RetryClient<MultiProvider<Http>>>;

const RPC_URL: &str = "https://eth.llamarpc.com";

// The MultiProvider here is our custom provider that allows us
// to switch between different RPCs in runtime to ensure we
// do not hit any rate-limits or errors.
let providers = vec![Http::new(RPC_URL)];
let multi_provider = MultiProvider::new(Arc::new(providers));
let retry_client = RetryClientBuilder::default().timeout_retries(u32::MAX).rate_limit_retries(u32::MAX).build(multi_provider, WebbHttpRetryPolicy::boxed());
let provider: EthersClient = Provider::new(retry_client).interval(Duration::from_secs(12));

let block_confirmations = 20;
let client = Arc::new(TimeLag::new(client.clone(), block_confirmations));

let config = /* ... */;
let store = SledStore::temporary()?;
let ctx = RelayerContext::new(config, store.clone())?;
```

Now after that boilerplate, we are ready to implemented our `EventWatcher` trait for `USDCContractWatcher`.

```rust
/// USDC Contract Watcher
#[derive(Copy, Clone, Debug, Default)]
struct USDCContractWatcher;

#[async_trait::async_trait]
impl EventWatcher for USDCContractWatcher {
    // Useful for logging, if you are using `tracing` crate.
    const TAG: &'static str = "USDC Contract Watcher";
    // a contract that implements WatchableContract trait.
    type Contract = USDC;
    // The events that this contract will be watching
    type Events = USDCEvents;
    // a store used to save state across runs.
    type Store = SledStore;
}
```

We can now start the event watcher like so:

```rust
let contract = USDC::new("0x7EA2be2df7BA6E54B1A9C70676f668455E329d29", client.clone());
let contract_watcher = USDCContractWatcher::default();
EventWatcher::run(
  &contract_watcher,
  client.clone(),
  store.clone(),
  contract,
  // this holds the event handlers
  // for now, we do not have any event handlers
  vec![],
  &ctx,
).await?;
```

This will start running the USDC event watcher by going through the blocks and parsing the events and then calling the handlers to handle these events, lets write a basic event handler that just prints the amount of USDC that gets transfered.

```rust
#[derive(Copy, Clone, Debug, Default)]
pub struct USDCTransferLogger;

#[async_trait::async_trait]
impl EventHandler for USDCTransferLogger {
    type Contract = USDC;

    type Events = USDCEvents;

    type Store = SledStore;
    // This method gets called first, to check if
    // this event handler will be able to handle this type of events
    // or not.
    async fn can_handle_events(
        &self,
        (events, _meta): (Self::Events, LogMeta),
        _wrapper: &Self::Contract,
    ) -> webb_relayer_utils::Result<bool> {
        use USDCEvents::*;
        let has_event = matches!(events, TransferFilter(_));
        Ok(has_event)
    }

    #[tracing::instrument(
        skip_all,
        fields(event_type = ?e.0),
    )]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        e: (Self::Events, LogMeta),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        let event = e.0;
        tracing::debug!(?event, "Got an event");
        Ok(())
    }
}
```

Updating our Event watcher to include this event handler

```rust
let tansfer_event_logger = USDCTransferLogger::default();

// ...snip..

EventWatcher::run(
  &contract_watcher,
  client.clone(),
  store.clone(),
  contract,
  vec![Box::new(tansfer_event_logger)],
  &ctx,
).await?;
```

Whenever we see a `Transfer` event, the event watcher will call the event handler to handle these kind of events.

Now lets go back and implement our example, the "how many USDC got transfeered in the last 24h" event handler.

```rust
#[derive(Copy, Clone, Debug, Default)]
pub struct USDC24HourTransferHandler;

#[async_trait::async_trait]
impl EventHandler for USDC24HourTransferHandler {
    type Contract = USDC;

    type Events = USDCEvents;

    type Store = SledStore;
    // This method gets called first, to check if
    // this event handler will be able to handle this type of events
    // or not.
    async fn can_handle_events(
        &self,
        (events, _meta): (Self::Events, LogMeta),
        _wrapper: &Self::Contract,
    ) -> webb_relayer_utils::Result<bool> {
        use USDCEvents::*;
        let has_event = matches!(events, TransferFilter(_));
        Ok(has_event)
    }

    #[tracing::instrument(
        skip_all,
        fields(event_type = ?e.0),
    )]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        e: (Self::Events, LogMeta),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        let event = e.0;
        // Left for the reader.
        Ok(())
    }
}

```

And later on you can register this handler too, to the list of event handlers:

```rust
let tansfer_event_logger = USDCTransferLogger::default();
let vol_event_handler = USDC24HourTransferHandler::default();

// ...snip..

EventWatcher::run(
  &contract_watcher,
  client.clone(),
  store.clone(),
  contract,
  vec![Box::new(tansfer_event_logger), Box::new(vol_event_handler)],
  &ctx,
).await?;
```
