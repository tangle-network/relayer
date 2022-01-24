import {
  JsNoteBuilder,
  JsNote,
  ProofInputBuilder,
} from '@webb-tools/wasm-utils/njs/wasm-utils';
import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring, decodeAddress } from '@polkadot/keyring';
import { KeyringPair } from '@polkadot/keyring/types';
import { u8aToHex } from '@polkadot/util';
import {
  getRelayerConfig,
  RelayerChainConfig,
  sleep,
  startWebbRelayer,
} from '../../relayerUtils';
import { ChildProcessWithoutNullStreams } from 'child_process';
import WebSocket from 'ws';

let apiPromise: ApiPromise | null = null;
let keyring: {
  bob: KeyringPair;
  alice: KeyringPair;
  charlie: KeyringPair;
} | null = null;
let relayer: ChildProcessWithoutNullStreams;
let relayerEndpoint: string;

let relayerChain1Info: RelayerChainConfig;
let relayerChain2Info: RelayerChainConfig;
let client: WebSocket;

async function preparePolkadotApi() {
  const wsProvider = new WsProvider('ws://127.0.0.1:9944');
  const api = await ApiPromise.create({
    provider: wsProvider,
    rpc: {
      mt: {
        getLeaves: {
          description: 'Query for the tree leaves',
          params: [
            {
              name: 'tree_id',
              type: 'u32',
              isOptional: false,
            },
            {
              name: 'from',
              type: 'u32',
              isOptional: false,
            },
            {
              name: 'to',
              type: 'u32',
              isOptional: false,
            },
            {
              name: 'at',
              type: 'Hash',
              isOptional: true,
            },
          ],
          type: 'Vec<[u8; 32]>',
        },
      },
    },
  });
  return api.isReady;
}

const BOBPhrase =
  'asthma early danger glue satisfy spatial decade wing organ bean census announce';

function getKerings() {
  if (keyring) {
    return keyring;
  }
  const k = new Keyring({ type: 'sr25519' });
  const bob = k.addFromMnemonic(BOBPhrase);
  const alice = k.addFromUri('//Alice');
  const charlie = k.addFromUri('//Charlie');
  keyring = {
    bob,
    alice,
    charlie,
  };
  return keyring;
}

async function transfareBalance(api: ApiPromise) {
  const { charlie, bob, alice } = getKerings();
  // transfer to alice
  // @ts-ignore
  await api.tx.balances
    .transfer(alice.address, 100_000_000_000_000)
    .signAndSend(charlie, {
      nonce: -1,
    });
  // transfer to test accounts
  // @ts-ignore
  await api.tx.balances
    .transfer(bob.address, 100_000_000_000_000)
    .signAndSend(charlie, {
      nonce: -1,
    });
}

// @ts-ignore
async function sendWebbtoken(api: ApiPromise, receiver: KeyringPair) {
  const { alice: sudoPair } = getKerings();
  return new Promise((resolve, reject) => {
    const address = api.createType('MultiAddress', { Id: receiver.address });
    // @ts-ignore
    api.tx.sudo
      .sudo(
        // @ts-ignore
        api.tx.currencies.updateBalance(address, 0, 100_000_000_000_000)
      )
      .signAndSend(sudoPair, (res) => {
        if (res.isFinalized || res.isCompleted) {
          resolve(null);
        }
        if (res.isError) {
          reject(res.dispatchError);
        }
      });
  });
}

async function depositMixerBnX5_5(api: ApiPromise, depositer: KeyringPair) {
  let noteBuilder = new JsNoteBuilder();
  noteBuilder.prefix('webb.mixer');
  noteBuilder.version('v1');

  noteBuilder.sourceChainId('1');
  noteBuilder.targetChainId('1');

  noteBuilder.tokenSymbol('WEBB');
  noteBuilder.amount('1');
  noteBuilder.denomination('18');

  noteBuilder.backend('Arkworks');
  noteBuilder.hashFunction('Poseidon');
  noteBuilder.curve('Bn254');
  noteBuilder.width('5');
  noteBuilder.exponentiation('5');
  const note = noteBuilder.build();
  const leaf = note.getLeafCommitment();
  //@ts-ignore
  const depositTx = api.tx.mixerBn254.deposit(0, leaf);
  await depositTx.signAndSend(depositer);
  return note;
}
async function fetchTreeLeaves(
  api: ApiPromise,
  treeId: string | number
): Promise<Uint8Array[]> {
  let done = false;
  let from = 0;
  let to = 511;
  const leaves: Uint8Array[] = [];

  while (done === false) {
    const treeLeaves: any[] = await (api.rpc as any).mt.getLeaves(
      treeId,
      from,
      to
    );
    if (treeLeaves.length === 0) {
      done = true;
      break;
    }
    leaves.push(...treeLeaves.map((i) => i.toU8a()));
    from = to;
    to = to + 511;
  }
  return leaves;
}
async function withdrawMixerBnX5_5(
  api: ApiPromise,
  signer: KeyringPair,
  note: JsNote
) {
  const accountId = signer.address;
  const addressHex = u8aToHex(decodeAddress(accountId));
  // fetch leaves
  const leaves = await fetchTreeLeaves(api, 0);
  const proofBuilder = new ProofInputBuilder();
  const leafHex = u8aToHex(note.getLeafCommitment());
  proofBuilder.setNote(note);
  proofBuilder.setLeaves(leaves);
  const leafIndex = leaves.findIndex((l) => u8aToHex(l) === leafHex);
  proofBuilder.setLeafIndex(String(leafIndex));

  proofBuilder.setFee('0');
  proofBuilder.setRefund('0');

  proofBuilder.setRecipient(addressHex.replace('0x', ''));
  proofBuilder.setRelayer(addressHex.replace('0x', ''));

  proofBuilder.setPk('');
  const proof = proofBuilder.build_js();
}

describe('Mixer tests', function () {
  // increase the timeout for relayer tests
  this.timeout(120_000);

  before(async function () {
    [relayer, relayerEndpoint] = await startWebbRelayer(8888);
    await sleep(1500); // wait for the relayer start-up
    relayerChain1Info = await getRelayerConfig('testa', relayerEndpoint);
    relayerChain2Info = await getRelayerConfig('testb', relayerEndpoint);
    apiPromise = await preparePolkadotApi();
    client = new WebSocket(`${relayerEndpoint.replace('http', 'ws')}/ws`);
    await new Promise((resolve) => client.on('open', resolve));
    console.log('Connected to Relayer!');
  });
  it('should relay successfully', async function () {
    const { bob } = getKerings();
    await transfareBalance(apiPromise!);
    await sendWebbtoken(apiPromise!, bob);
    const note = await depositMixerBnX5_5(apiPromise!, bob);
    await withdrawMixerBnX5_5(apiPromise!, bob, note);
  });
});
