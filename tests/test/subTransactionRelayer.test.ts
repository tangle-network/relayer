// This our basic Substrate Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just relay transactions for us.

import { expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import isCi from 'is-ci';
import child from 'child_process';
import { WebbRelayer } from '../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../lib/localProtocolSubstrate.js';
import { UsageMode } from '../lib/substrateNodeBase.js';
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
  this.timeout(60000);
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;

  let webbRelayer: WebbRelayer;

  before(async () => {
    const usageMode: UsageMode = isCi
      ? { mode: 'docker', forcePullImage: false }
      : {
          mode: 'host',
          nodePath: path.resolve(
            '../../protocol-substrate/target/release/webb-standalone-node'
          ),
        };

    // for manual connection
    const aliceManualPorts = {
      ws: 9944,
      http: 9933,
      p2p: 30333
    }

    // for manual connection
    const bobManualPorts = {
      ws: 9945,
      http: 9934,
      p2p: 30334
    }

    aliceNode = await LocalProtocolSubstrate.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: aliceManualPorts,
      isManual: true
    });

    bobNode = await LocalProtocolSubstrate.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode,
      ports: bobManualPorts,
      isManual: true
    });

    await aliceNode.writeConfig({
      path: `${tmpDirPath}/${aliceNode.name}.json`,
      suri: '//Charlie',
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

  it('Simple Mixer Transaction', async () => {
    const api = await aliceNode.api();
    const { tx, note } = await createMixerDepositTx(api);
    const keyring = new Keyring({ type: 'sr25519' });
    const charlie = keyring.addFromUri('//Charlie');
    // send the deposit transaction.
    const txSigned = tx.sign(charlie);
    await aliceNode.executeTransaction(txSigned);
    // next we need to prepare the withdrawal transaction.
    const withdrawalProof = await createMixerWithdrawProof(api, note, {
      recipient: charlie.address,
      relayer: charlie.address,
      fee: 0,
      refund: 0,
    });
    // ping the relayer!
    await webbRelayer.ping();

    // get the initial balance
    // @ts-ignore
    let { nonce, data: balance } = await api.query.system.account(charlie.address);
    let initialBalance = balance.free.toBigInt();
    console.log(`balance before withdrawal is ${balance.free.toBigInt()}`);
    // now we need to submit the withdrawal transaction.
    const txHash = await webbRelayer.substrateMixerWithdraw({
      chain: aliceNode.name,
      id: withdrawalProof.id,
      proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
      root: Array.from(hexToU8a(withdrawalProof.root)),
      nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
      refund: withdrawalProof.refund,
      fee: withdrawalProof.fee,
      recipient: withdrawalProof.recipient,
      relayer: withdrawalProof.relayer,
    });
    expect(txHash).to.be.not.null;

    // get the balance after withdrawal is done and see if it increases
    // @ts-ignore
    const { nonce: nonceAfter, data: balanceAfter } = await api.query.system.account(charlie.address);
    let balanceAfterWithdraw = balanceAfter.free.toBigInt();
    console.log(`balance after withdrawal is ${balanceAfter.free.toBigInt()}`);
    expect(balanceAfterWithdraw > initialBalance).true;
  });

  after(async () => {
    await aliceNode?.stop();
    await bobNode?.stop();
    await webbRelayer?.stop();
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
  note: any,
  opts: WithdrawalOpts
): Promise<WithdrawalProof> {
  try {
    const recipientAddressHex = u8aToHex(decodeAddress(opts.recipient)).replace(
      '0x',
      ''
    );
    const relayerAddressHex = u8aToHex(decodeAddress(opts.relayer)).replace(
      '0x',
      ''
    );
    const treeId = 0;
    //@ts-ignore
    const getLeaves = api.rpc.mt.getLeaves;
    const treeLeaves: Uint8Array[] = await getLeaves(treeId, 0, 500);
    const pm = new ProvingManagerWrapper('direct-call');
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
      proofBytes: `0x${zkProof.proof}`,
      root: `0x${zkProof.root}`,
      nullifierHash: `0x${zkProof.nullifierHash}`,
      recipient: opts.recipient,
      relayer: opts.relayer,
      fee: opts.fee,
      refund: opts.refund,
    };
  } catch (error) {
    //@ts-ignore
    console.error(error.error_message);
    //@ts-ignore
    console.error(error.code);
    throw error;
  }
}
