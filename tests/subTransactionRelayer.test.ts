// This our basic EVM Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just relay transactions for us.

import { jest } from '@jest/globals';
import 'jest-extended';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import child from 'child_process';
import getPort, { portNumbers } from 'get-port';
import { WebbRelayer } from './lib/webbRelayer';
import { LocalProtocolSubstrate } from './lib/localProtocolSubstrate';
import {
  generate_proof_js,
  JsNote,
  JsNoteBuilder,
  ProofInputBuilder,
} from '@webb-tools/wasm-utils/njs';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { decodeAddress } from '@polkadot/util-crypto';

describe('Substrate Transaction Relayer', () => {
  const tmp = temp.track();
  jest.setTimeout(120_000);
  const tmpDirPath = tmp.mkdirSync({ prefix: 'webb-relayer-test-' });
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;

  let webbRelayer: WebbRelayer;

  beforeAll(async () => {
    aliceNode = await LocalProtocolSubstrate.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode: { mode: 'docker', forcePullImage: false },
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
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
    });
    await webbRelayer.waitUntilReady();
  });

  test('Simple Mixer Transaction', async () => {
    const api = await aliceNode.api();
    const { tx, note } = createMixerDepositTx(api);
    const keyring = new Keyring({ type: 'sr25519' });
    const alice = keyring.addFromUri('//Alice');
    // send the deposit transaction.
    await tx.signAndSend(alice, { nonce: -1 }, (res) => {
      if (res.status.isFinalized) {
        expect(res.isError).toBeFalsy();
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
    expect(txHash).toBeDefined();
  });

  afterAll(async () => {
    await aliceNode.stop();
    await bobNode.stop();
    await webbRelayer.stop();
    tmp.cleanupSync(); // clean up the temp dir.
  });
});

// Helper methods, we can move them somewhere if we end up using them again.

function createMixerDepositTx(api: ApiPromise): {
  tx: SubmittableExtrinsic<'promise'>;
  note: JsNote;
} {
  const noteBuilder = new JsNoteBuilder();
  noteBuilder.protocol('mixer');
  noteBuilder.version('v2');

  noteBuilder.sourceChainId('1');
  noteBuilder.targetChainId('1');
  noteBuilder.sourceIdentifyingData('3');
  noteBuilder.targetIdentifyingData('3');

  noteBuilder.tokenSymbol('WEBB');
  noteBuilder.amount('1');
  noteBuilder.denomination('18');

  noteBuilder.backend('Arkworks');
  noteBuilder.hashFunction('Poseidon');
  noteBuilder.curve('Bn254');
  noteBuilder.width('3');
  noteBuilder.exponentiation('5');
  const note = noteBuilder.build();
  const leaf = note.getLeafCommitment();
  const treeId = 0;
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
  note: JsNote,
  opts: WithdrawalOpts
): Promise<WithdrawalProof> {
  const recipientAddressHex = u8aToHex(decodeAddress(opts.recipient));
  const relayerAddressHex = u8aToHex(decodeAddress(opts.relayer));
  const treeId = 0;
  //@ts-ignore
  const getLeaves = api.rpc.mt.getLeaves;
  const treeLeaves: Uint8Array[] = await getLeaves(treeId, 0, 500);
  const proofInputBuilder = new ProofInputBuilder();
  const leafHex = u8aToHex(note.getLeafCommitment());
  const leafIndex = treeLeaves.findIndex((l) => u8aToHex(l) === leafHex);
  expect(leafIndex).toBeGreaterThan(-1);
  proofInputBuilder.setNote(note);
  proofInputBuilder.setLeaves(treeLeaves);
  proofInputBuilder.setLeafIndex(leafIndex.toString());
  proofInputBuilder.setFee(opts.fee.toString());
  proofInputBuilder.setRefund(opts.refund.toString());
  proofInputBuilder.setRecipient(recipientAddressHex.replace('0x', ''));
  proofInputBuilder.setRelayer(relayerAddressHex.replace('0x', ''));
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
  proofInputBuilder.setPk(provingKey.toString('hex'));

  const proofInput = proofInputBuilder.build_js();
  const zkProofMetadata = generate_proof_js(proofInput);
  return {
    id: treeId,
    proofBytes: `0x${zkProofMetadata.proof}`,
    root: `0x${zkProofMetadata.root}`,
    nullifierHash: `0x${zkProofMetadata.nullifierHash}`,
    recipient: opts.recipient,
    relayer: opts.relayer,
    fee: opts.fee,
    refund: opts.refund,
  };
}
