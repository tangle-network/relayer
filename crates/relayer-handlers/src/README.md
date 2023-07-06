## Relayer API Documentation

The relayer has 8 endpoints available to query from. They are outlined below for your convenience.

---

**Retrieving nodes IP address:**
Returns Ip address of the relayer.
- URL : `/api/v1/ip`
- Method: `GET`

##### Response

```json
{
    "ip": "127.0.0.1"
}
```

---
**Retrieve relayer configuration**
Returns relayer configuration.
- URL : `/api/v1/info`
- Method : `GET`
```
/api/v1/info
```

<details>
  <summary>Expected Response</summary>
  
  ```json
  {
    "evm": {
        "rinkeby": {
            "enabled": true,
            "chainId": 4,
            "beneficiary": "0x58fcd47ece3ed24ace88fee06efd90dcb38f541f",
            "contracts": [{
                "contract": "Anchor",
                "address": "0x9d36b94f245857ec7280415140800dde7642addb",
                "deployedAt": 8896800,
                "eventsWatcher": {
                    "enabled": true,
                    "pollingInterval": 15000
                },
                "size": 0.1,
                "proposalSigningBackend": { "type": "DKGNode", "node": "dkg-local" },
                "withdrawFeePercentage": 0.05
            }]
        }
    },
    "substrate": {},
    "experimental": {
        "smart-anchor-updates": false,
        "smart-anchor-updates-retries": 0
    },
    "features": { "dataQuery": true, "governanceRelay": true, "privateTxRelay": true },
    "assets": {
        "TNT": { "price": 0.1, "name": "Tangle Network Token", "decimals": 18 },
        "tTNT": { "price": 0.1, "name": "Test Tangle Network Token", "decimals": 18 }
    },
    "build" : {
        "version": "0.5.0",
        "commit": "c8875ba78298d34272e40c2e302fcfe33f191147",
        "timestamp": "2023-05-19T15:57:40Z"
    }
}
  ```
</details>

---

**Retrieve historical leaves cache**
Return commitment leaves cached by the relayer.
- URL : `/api/v1/leaves/evm/:chain_id/:contract_address`
- Method : `GET`

##### Parameters

- `chain_id`: ChainId of the system
- `contract_address` Contract address of `vanchor` system.

##### Example

```
/api/v1/leaves/evm/4/0x9d36b94f245857ec7280415140800dde7642addb
```

<details>
  <summary>Expected Response</summary>
  
  ```json
{
  "leaves": [
    "0x015110a4b1a8bf29f7b6b2cb3fe5f52c2eeccd9ff7e8a0fb7d4ff2ae61516562",
    "0x2fa56e6179d1bf0afc6f3ee2a52dc68cc2076d380a55165578c1c558e1f6f1dc",
    "0x031317e0fe026ce99cf9b3cf8fefed7ddc21c5f4181e49fd6e8370aea5006da0",
    "0x07507826af3c90c457222ad0305d90bf8bcfb1d343c2a9c17d280ff648b43582",
    "0x0ff8f7f0fc798b9b34464ba51a10bdde16d17506f3251f9658335504f07c9c5f",
    "0x0b92b3c5013eb2374527a167af6464f1ab8b11da1dd36e5a6a2cf76130fee9e3",
    "0x2bccea444d1078a2b5778f3c8f28013219abfe5c236d1276b87276ec5eec4354",
    "0x0be7c8578e746b1b7d913c79affb80c715b96a1304edb68d1d7e6dc33f30260f",
    "0x117dae7ac7b62ed97525cc8541823c2caae25ffaf6168361ac19ca484851744f",
    "0x0c187c0b413f2c2e8ebaeffbe9351fda6eb46dfa396b0c73298215950439fa75"
  ],
  "lastQueriedBlock": 37
}

```
</details>

---

**Retrieve encrypted output cache**
Returns encrypted outputs cached by the relayer.
- URL : `/api/v1/encrypted_outputs/evm/:chain_id/:contract_address`
- Method : GET

##### Example
```
/api/v1/encrypted_outputs/evm/4/0x9d36b94f245857ec7280415140800dde7642addb
```

<details>
  <summary>Expected Response</summary>

  ```json
   {
    "encryptedOutputs": [
     [
      "5d7af1ca18064f45e05f4a74191603dc9e6547a848892ca424c0a14f796e33bcc4dbec9c312d50334be022afcdc48c3462e8f3897b18de4a4d9a32d58725c3b3c0aaedb34528ad3e4aa2e788801c798d72726ff8220a848c5e8b80d6d674f37930531a99a0375667ce9a04d0264a9ae32dd0079d6e37b1cefbb3c723787d58bdab1c53dfd868ff2387b7ff61e5ab23f7c99ea8f815feef2debb967fcfae8d23a240ee031d567c80f", "06ac71ded9713e44c9dce1562a280d286ef6acbfe64cb89046ffbc5ee26cf65de7f6c6b1064f94330481eb81ce0718a5d370433b52790281b0fb8c7ca1b09ea9efb9bcc5b06b6231c890e0d6aaa08d5a35e8a3a103bdac1bb6680ef0410ace5f1472955fbe9a87c2c909ef6b65e1da4e714d5c622b465781ea91a40cf3697f873c6e7b7dd7876daec511d34742843720473e6e67aac4f87fc31df1d918be9618cbeb5e124a3a4a6f"
      ]
    ],
    "lastQueriedBlock": 37
   }
```
</details>

---

**Retrieve fee information**
Returns estimated fee and max refund amount before making withdrawal request to relayer
- URL : `/fee_info/evm/:chain_id/:vanchor/:gas_amount`
- Method : `GET`

##### Parameters

- `chain_id`: ChainId of the system
- `contract_address`: Contract address of `vanchor` system.
- `gas_amount`: Gas amount 

##### Response
```json
{
  "estimatedFee": "0x476b26e0f",
  "gasPrice": "0x11",
  "refundExchangeRate": "0x28f",
  "maxRefund": "0xf3e59",
  "timestamp": "2023-01-19T06:29:49.556114073Z"
}
```


---

**Retrieve Metrics information**
Returns relayer metrics
- URL : `/api/v1/metrics`
- Method : `GET

<details>
  <summary>Expected Response</summary>

```json
{
  "metrics": "# HELP bridge_watcher_back_off_metric specifies how many times the bridge watcher backed off\n# TYPE bridge_watcher_back_off_metric counter\nbridge_watcher_back_off_metric 0\n# HELP gas_spent_metric The total number of gas spent\n# TYPE gas_spent_metric counter\ngas_spent_metric 0\n# HELP handle_proposal_execution_metric How many times did the function handle_proposal get executed\n# TYPE handle_proposal_execution_metric counter\nhandle_proposal_execution_metric 0\n# HELP proposal_queue_attempt_metric How many times a proposal is attempted to be queued\n# TYPE proposal_queue_attempt_metric counter\nproposal_queue_attempt_metric 0\n# HELP total_active_relayer_metric The total number of active relayers\n# TYPE total_active_relayer_metric counter\ntotal_active_relayer_metric 0\n# HELP total_fee_earned_metric The total number of fees earned\n# TYPE total_fee_earned_metric counter\ntotal_fee_earned_metric 0\n# HELP total_number_of_data_stored_metric The Total number of data stored\n# TYPE total_number_of_data_stored_metric counter\ntotal_number_of_data_stored_metric 1572864\n# HELP total_number_of_proposals_metric The total number of proposals proposed\n# TYPE total_number_of_proposals_metric counter\ntotal_number_of_proposals_metric 0\n# HELP total_transaction_made_metric The total number of transaction made\n# TYPE total_transaction_made_metric counter\ntotal_transaction_made_metric 0\n# HELP transaction_queue_back_off_metric How many times the transaction queue backed off\n# TYPE transaction_queue_back_off_metric counter\ntransaction_queue_back_off_metric 0\n"
}
```
</details>

---

**Retrieve Metrics information for specific target system**
Returns metrics for particular target system or resource.
- URL : /api/v1/metrics/evm/:chain_id/:contract_address
- Method : `GET`

##### Parameters

- `chain_id`: ChainId of the system
- `contract_address`: Contract address of `vanchor` system.

##### Example

```
/api/v1/metrics/evm/4/0x9d36b94f245857ec7280415140800dde7642addb
```

##### Response
```json
{ 
    "totalGasSpent": "1733870",
    "totalFeeEarned": "1787343976",
    "accountBalance": "10000003900094" 
}
```
---

**Send withdraw transaction request to relayer**
You can make a withdraw request to relayer to withdraw amount privately. This api will return an `item_key` which can be used to track transaction progress.

- URL : `/api/v1/send/evm/:chain_id/:contract_address`
- Method : `POST`
- Returns : `item_key`


##### Parameters

- `chain_id`: ChainId of the system
- `contract_address`: Contract address of `vanchor` system.

##### Request Payload
```json
{
  "extData": {
    "recipient": "0x61f87418b7F93B242FC349a18901511719840f8A",
    "relayer": "0xC1b634853Cb333D3aD8663715b08f41A3Aec47cc",
    "extAmount": "-32e976941b78c20c6d",
    "fee": "0x00000000000000034c5319aa65ddf393",
    "refund": "0x00000000000000000000000000000000",
    "token": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
    "encryptedOutput1": "0x84e190004e5a43dde02d3f172fb54a1171cb04e7599baa4b98666df3ca2a717a626f0828e23f9d84278786034793c5a6ef9fe8b3b6595c2e2224f5baa9dcb170ef79a2c6a5604d7d18995d6758a8140abd71c7cb7dde469b132ea3fedbfd33363672224f2dd7923ae681cab59fb24aa6e1e18edb3b58c48ab32b6fa5c25d5bbde58854945f6fb3d150fb0aeeaa313a54540de6ecc5556d417464df959214956f983e0c399666637c",
    "encryptedOutput2": "0x5aace9d7c642b3a67ed92d266e5b5b4c51581b3cef1d2869ca24a6971752d198624a66e5c456db7e7009b7727980ddc278e82c4287c8e81eebb06a4c13a11669382bfe24253c78bc6919ea5864b2cf3369d90bc8faa618c69a97aeee7ddafbe1f1753c5339face258d129565771ad5681ff76c6fef0586823d307098ea9bea68883fdb33fa11afbb67427bc0014dbcdf01b9498f49b3dd62d613dc09764a2a56df97c903eb68571f"
  },
  "proofData": {
    "proof": "0x107210365314564a7e2cfe0471f8e3a3390b01796c843d7d5f4973d6621654e1246f4ff88fc44c91c392491bd0ba603dd7b6b459664882c6d2a5fa1e4f3f87df2d14edc4999a85755d12f12703544f9b00b964781c086317059a06b07d5906d321cdbb2c9fbd87a87c1de18940c5b2ca9419f80686eb1ea784736e98d995f8371fab9e8b65e0faf6923db10158477fc8d647e049e5e8ecdb0540031f7edbac2426fbaa73bd0bb2a7690a5778301313c17549edbd4fad5c6a65e30c788d681d350b5fd5bdd5ad451f2cdc9ae33a4473cfc697cec2925eb3303e3c33ad6e45d6211ce6d2bbf973bc8a71f74c7a0fdc9327ff45edda4eb8c2f261484121f05568d1",
    "extDataHash": "0x0ba90a0ff6fbb0b35b99f5828bb17a9a90d9d4608973294d2b9d885c411325bd",
    "publicAmount": "0x30644e72e131a029b85045b68181585d2833e84879b9705b0e1847ce11600001",
    "roots": [
      "0x0a10b873a48008d5d39808fab818591815ce2a83b20ad384a6bb26474a4dbc37",
      "0x23ab323453748129f2765f79615022f5bebd6f4096a796300aab049a60b0f187"
    ],
    "extensionRoots": [],
    "outputCommitments": [
      "0x09bcfab435d5e9bbfb3cc5ad879d8354700a0a3aa6d5a5a247c9264e5e03c0b0",
      "0x202fc1611fbb99fcec34ae1362185d9ce1cafc1d9b53a636b3a530a29015f4c6"
    ],
    "inputNullifiers": [
      "0x270ac8f4c30e21372a0b692617788c8d2da0cc6864513f6d5225c57ffcd2062a",
      "0x02ad7bfd630e3965db175940d38f2e8f74b38e387d1daf5aa2d725aef363dd08"
    ]
  }
}
```

##### Response
```json
{
  "status": "Sent",
  "message": "Transaction sent successfully",
  "itemKey": "0x65766d5f7472616e73616374696f6e5f71756575655f6974656d5f6b65795f5f653e1f954f5d2b89943baccce52982c71e263da5f2d3a5fea9ea35ec312e00b8"
}
```

---

**Track transaction item progress**
Returns transaction item progress for given `item_key`
- URL : `/api/v1/tx/evm/:chain_id:/:item_key`
- Method : `GET`

##### Parameters

- `chain_id`: ChainId of the system
- `item_key` : An 64 byte hex string.

##### Response
```json
{
  "status": "Pending",
  "itemKey": "0x7375…58ac"
}
```





