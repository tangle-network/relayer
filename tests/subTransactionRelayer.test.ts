// This our basic Substrate Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just relay transactions for us.

import { expect } from 'chai';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import child from 'child_process';
import getPort, { portNumbers } from 'get-port';
import { WebbRelayer } from './lib/webbRelayer.js';
import { LocalProtocolSubstrate } from './lib/localProtocolSubstrate.js';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { decodeAddress } from '@polkadot/util-crypto';
import {
  Note,
  NoteGenInput,
  ProvingManagerSetupInput,
  ProvingManagerWrapper,
} from '@webb-tools/sdk-core';

describe('Substrate Transaction Relayer', function () {
  const tmp = temp.track();
  const tmpDirPath = tmp.mkdirSync({ prefix: 'webb-relayer-test-' });
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;

  let webbRelayer: WebbRelayer;

  before(async () => {
    aliceNode = await LocalProtocolSubstrate.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode: { mode: 'host', nodePath: '' },
      ports: 'auto',
    });

    bobNode = await LocalProtocolSubstrate.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode: { mode: 'docker', forcePullImage: false },
      ports: 'auto',
    });

    await aliceNode.writeConfig({
      path: `${tmpDirPath}/${aliceNode.name}.json`,
      suri: '//Alice',
    });

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(8000, 8888) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
    });
    await webbRelayer.waitUntilReady();
  });

  test('Simple Mixer Transaction', async () => {
    const api = await aliceNode.api();
    const { tx, note } = await createMixerDepositTx(api);
    const keyring = new Keyring({ type: 'sr25519' });
    const alice = keyring.addFromUri('//Alice');
    // send the deposit transaction.
    await tx.signAndSend(alice, { nonce: -1 }, (res) => {
      console.log(res.status.toHuman());
      if (res.status.isFinalized) {
        expect(res.isError).to.be.false;
      }
    });
    await aliceNode.waitForEvent({ section: 'mixerBn254', method: 'Deposit' });
    // next we need to prepare the withdrawal transaction.
    const withdrawalProof = await createMixerWithdrawProof(api, note, {
      recipient: alice.address,
      relayer: alice.address,
      fee: 0,
      refund: 0,
    });
    // ping the relayer!
    await webbRelayer.ping();
    // now we need to submit the withdrawal transaction.
    const txHash = await webbRelayer.substrateMixerWithdraw({
      chain: aliceNode.name,
      id: withdrawalProof.id,
      proof: withdrawalProof.proofBytes as any,
      root: hexToU8a(withdrawalProof.root),
      nullifierHash: hexToU8a(withdrawalProof.nullifierHash),
      refund: withdrawalProof.refund,
      fee: withdrawalProof.fee,
      recipient: withdrawalProof.recipient,
      relayer: withdrawalProof.relayer,
    });
    expect(txHash).to.be.not.null;
  });

  after(async () => {
    await aliceNode.stop();
    await bobNode.stop();
    await webbRelayer.stop();
    tmp.cleanupSync(); // clean up the temp dir.
  });
});

// Helper methods, we can move them somewhere if we end up using them again.

async function createMixerDepositTx(api: ApiPromise): Promise<{
  tx: SubmittableExtrinsic<'promise'>;
  note: Note;
}> {
  const noteInput: NoteGenInput = {
    protocol: 'mixer',
    version: 'v2',
    sourceChain: '5',
    targetChain: '5',
    sourceIdentifyingData: '3',
    targetIdentifyingData: '3',
    tokenSymbol: 'WEBB',
    amount: '1',
    denomination: '18',
    backend: 'Arkworks',
    hashFunction: 'Poseidon',
    curve: 'Bn254',
    width: '3',
    exponentiation: '5',
  };
  const note = await Note.generateNote(noteInput);
  const treeId = 0;
  const leaf = note.getLeaf();
  const tx = api.tx.mixerBn254!.deposit!(treeId, leaf);
  return { tx, note };
}

type WithdrawalOpts = {
  relayer: string;
  recipient: string;
  fee: number;
  refund: number;
};

type WithdrawalProof = {
  id: number;
  proofBytes: string;
  root: string;
  nullifierHash: string;
  recipient: string;
  relayer: string;
  fee: number;
  refund: number;
};

async function createMixerWithdrawProof(
  api: ApiPromise,
  note: Note,
  opts: WithdrawalOpts
): Promise<WithdrawalProof> {
  const recipientAddressHex = u8aToHex(decodeAddress(opts.recipient));
  const relayerAddressHex = u8aToHex(decodeAddress(opts.relayer));
  const treeId = 0;
  //@ts-ignore
  const getLeaves = api.rpc.mt.getLeaves;
  const treeLeaves: Uint8Array[] = await getLeaves(treeId, 0, 500);
  const pm = new ProvingManagerWrapper();
  const leafHex = u8aToHex(note.getLeaf());
  const leafIndex = treeLeaves.findIndex((l) => u8aToHex(l) === leafHex);
  expect(leafIndex).to.be.greaterThan(-1);
  const gitRoot = child
    .execSync('git rev-parse --show-toplevel')
    .toString()
    .trim();
  const provingKeyPath = path.join(
    gitRoot,
    'tests',
    'protocol-substrate-fixtures',
    'mixer',
    'bn254',
    'x5',
    'proving_key_uncompressed.bin'
  );
  const provingKey = fs.readFileSync(provingKeyPath);

  const proofInput: ProvingManagerSetupInput = {
    note: note.serialize(),
    relayer: relayerAddressHex,
    recipient: recipientAddressHex,
    leaves: treeLeaves,
    leafIndex,
    fee: opts.fee,
    refund: opts.refund,
    provingKey,
  };
  const zkProof = await pm.proof(proofInput);
  return {
    id: treeId,
    proofBytes: zkProof.proof,
    root: zkProof.root,
    nullifierHash: zkProof.nullifierHash,
    recipient: opts.recipient,
    relayer: opts.relayer,
    fee: opts.fee,
    refund: opts.refund,
  };
}
