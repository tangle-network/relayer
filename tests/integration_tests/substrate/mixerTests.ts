import { JsNote } from '@webb-tools/wasm-utils/njs/wasm-utils';
import { ApiPromise } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { KeyringPair } from '@polkadot/keyring/types';
import {
  getRelayerSubstrateConfig,
  handleMessage,
  KillTask,
  RelayerChainConfig,
  Result,
  sleep,
  startDarkWebbNode,
  startWebbRelayer,
} from '../../relayerUtils';
import { ChildProcessWithoutNullStreams } from 'child_process';
import WebSocket from 'ws';
import {
  depositMixerBnX5_5,
  preparePolkadotApi,
  setORMLTokenBalance,
  transferBalance,
  withdrawMixerBnX5_5,
} from '../../substrateUtils';

let apiPromise: ApiPromise | null = null;
let keyring: {
  bob: KeyringPair;
  alice: KeyringPair;
  charlie: KeyringPair;
} | null = null;
let relayer: ChildProcessWithoutNullStreams;
let nodes: KillTask | undefined;
let relayerEndpoint: string;

let relayerChain1Info: RelayerChainConfig;

let client: WebSocket;

const BOBPhrase =
  'asthma early danger glue satisfy spatial decade wing organ bean census announce';

function getKeyring() {
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

describe('Mixer tests', function () {
  // increase the timeout for relayer tests
  this.timeout(220_000);

  before(async function () {
    // If LOCAL_NODE is set the tests will continue  to use the already running node

      nodes = startDarkWebbNode();


    [relayer, relayerEndpoint] = await startWebbRelayer(8888);
    console.log(`Relayer PID ${relayer.pid}`);
    await sleep(1500); // wait for the relayer start-up
    relayerChain1Info = await getRelayerSubstrateConfig(
      'localnode',
      relayerEndpoint
    );

    apiPromise = await preparePolkadotApi();

    client = new WebSocket(`${relayerEndpoint.replace('http', 'ws')}/ws`);
    await new Promise((resolve) => client.on('open', resolve));
    console.log('Connected to Relayer!');
  });

  it('should relay successfully', async function () {
    const { bob, charlie, alice } = getKeyring();
    // transfer some funds to sudo & test account
    await transferBalance(apiPromise!, charlie, [alice, bob], 10_000);
    // set the test account ORML balance of the mixer asset
    await setORMLTokenBalance(apiPromise!, alice, bob);

    let note: JsNote;
    let req: any;
    // deposit to the mixer
    note = await depositMixerBnX5_5(apiPromise!, bob);
    ///Give the chain sometime to insure the leaf is there
    await sleep(10_000);
    // withdraw fro the mixer
    req = await withdrawMixerBnX5_5(
      apiPromise!,
      bob,
      note!,
      relayerChain1Info.beneficiary
    );

    if (client.readyState === client.OPEN) {
      const data = JSON.stringify(req);
      console.log('Sending Proof to the Relayer ..');
      console.log('=>', data);
      client.send(data, (err) => {
        console.log('Proof Sent!');
        if (err !== undefined) {
          console.log('!!Error!!', err);
          throw new Error('Client error sending proof');
        }
      });
    } else {
      console.error('Relayer Connection closed!');
      throw new Error('Client error, not OPEN');
    }

    await new Promise((resolve, reject) => {
      client.on('message', async (data) => {
        console.log('Received data from the relayer');
        console.log('<==', data);
        const msg = JSON.parse(data as string);
        const result = handleMessage(msg);
        if (result === Result.Errored) {
          reject(msg);
        } else if (result === Result.Continue) {
          // all good.
          return;
        } else if (result === Result.CleanExit) {
          console.log('Transaction Done and Relayed Successfully!');
          resolve(msg);
        }
      });
    });
  });

  after(function () {
    client?.terminate();
    relayer.kill('SIGINT');
    nodes?.();
  });
});
