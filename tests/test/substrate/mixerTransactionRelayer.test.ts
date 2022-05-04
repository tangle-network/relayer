// This our basic Substrate Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just relay transactions for us.

import { expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import isCi from 'is-ci';
import child from 'child_process';
import { WebbRelayer } from '../../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../../lib/localProtocolSubstrate.js';
import { UsageMode } from '../../lib/substrateNodeBase.js';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { ModuleErrors, SubmittableExtrinsic } from '@polkadot/api/types';
import { decodeAddress } from '@polkadot/util-crypto';
import {
  Note,
  NoteGenInput,
  ProvingManagerSetupInput,
  ProvingManagerWrapper,
} from '@webb-tools/sdk-core';

describe.skip('Substrate Mixer Transaction Relayer', function () {
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
      ports: 'auto',
    });

    bobNode = await LocalProtocolSubstrate.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode,
      ports: 'auto',
    });

    await aliceNode.writeConfig({
      path: `${tmpDirPath}/${aliceNode.name}.json`,
      suri: '//Charlie',
    });
    // Wait until we are ready and connected
    const api = await aliceNode.api();
    await api.isReady;
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
    const account = createAccount('//Dave');
    const note = await makeDeposit(api, aliceNode, account);
    const withdrawalProof = await initWithdrawal(
      api,
      webbRelayer,
      account,
      note
    );

    // get the initial balance
    // @ts-ignore
    let { nonce, data: balance } = await api.query.system.account(
      withdrawalProof.recipient
    );
    let initialBalance = balance.free.toBigInt();
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
    const { nonce: nonceAfter, data: balanceAfter } = await api.query.system!
      .account!(withdrawalProof.recipient);
    let balanceAfterWithdraw = balanceAfter.free.toBigInt();
    expect(balanceAfterWithdraw > initialBalance);
  });

  it('Should fail to withdraw if recipient address is invalid', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Dave');
    const note = await makeDeposit(api, aliceNode, account);
    const withdrawalProof = await initWithdrawal(
      api,
      webbRelayer,
      account,
      note
    );

    const invalidAddress = '5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PTXFy';

    // now we need to submit the withdrawal transaction.
    try {
      // try to withdraw with invalid address
      await webbRelayer.substrateMixerWithdraw({
        chain: aliceNode.name,
        id: withdrawalProof.id,
        proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
        root: Array.from(hexToU8a(withdrawalProof.root)),
        nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
        refund: withdrawalProof.refund,
        fee: withdrawalProof.fee,
        recipient: invalidAddress,
        relayer: withdrawalProof.relayer,
      });
    } catch (e) {
      // Expect an error to be thrown
      expect(e).to.not.be.null;
      // Runtime Error that indicates invalid withdrawal proof
      expect(e).to.contain(
        'Runtime error: RuntimeError(Module { index: 40, error: 1 }'
      );
    }
  });

  it('Should fail to withdraw if proof is invalid', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Eve');
    const note = await makeDeposit(api, aliceNode, account);
    const withdrawalProof = await initWithdrawal(
      api,
      webbRelayer,
      account,
      note
    );

    const proofBytes = hexToU8a(withdrawalProof.proofBytes);
    // flip a bit in the proof, so it is invalid
    const flipCount = proofBytes.length / 8;
    for (let i = 0; i < flipCount; i++) {
      proofBytes[i] |= 0x42;
    }
    const invalidProofBytes = u8aToHex(proofBytes);
    expect(withdrawalProof.proofBytes).to.not.eq(invalidProofBytes);

    // now we need to submit the withdrawal transaction.
    try {
      // try to withdraw with invalid address
      await webbRelayer.substrateMixerWithdraw({
        chain: aliceNode.name,
        id: withdrawalProof.id,
        proof: Array.from(hexToU8a(invalidProofBytes)),
        root: Array.from(hexToU8a(withdrawalProof.root)),
        nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
        refund: withdrawalProof.refund,
        fee: withdrawalProof.fee,
        recipient: withdrawalProof.recipient,
        relayer: withdrawalProof.relayer,
      });
    } catch (e) {
      // Expect an error to be thrown
      expect(e).to.not.be.null;
      // Runtime Error that indicates invalid withdrawal proof
      expect(e).to.contain('Module { index: 35, error: 1 }');
    }
  });

  it('Should fail to withdraw if fee is not expected', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Ferdie');
    const note = await makeDeposit(api, aliceNode, account);
    const withdrawalProof = await initWithdrawal(
      api,
      webbRelayer,
      account,
      note
    );

    const invalidFee = 100;

    // now we need to submit the withdrawal transaction.
    try {
      // try to withdraw with invalid address
      await webbRelayer.substrateMixerWithdraw({
        chain: aliceNode.name,
        id: withdrawalProof.id,
        proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
        root: Array.from(hexToU8a(withdrawalProof.root)),
        nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
        refund: withdrawalProof.refund,
        fee: invalidFee,
        recipient: withdrawalProof.recipient,
        relayer: withdrawalProof.relayer,
      });
    } catch (e) {
      // Expect an error to be thrown
      expect(e).to.not.be.null;
      // Runtime Error that indicates invalid withdrawal proof
      expect(e).to.contain(
        'Runtime error: RuntimeError(Module { index: 40, error: 1 }'
      );
    }
  });

  it('Should fail to withdraw with invalid root', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Eve');
    const note = await makeDeposit(api, aliceNode, account);
    const withdrawalProof = await initWithdrawal(
        api,
        webbRelayer,
        account,
        note
    );

    const rootBytes = hexToU8a(withdrawalProof.root);
    // flip a bit in the proof, so it is invalid
    const flipCount = rootBytes.length / 8;
    for (let i = 0; i < flipCount; i++) {
      rootBytes[i] |= 0x42;
    }
    const invalidRootBytes = u8aToHex(rootBytes);
    expect(withdrawalProof.proofBytes).to.not.eq(invalidRootBytes);

    // now we need to submit the withdrawal transaction.
    try {
      // try to withdraw with invalid address
      await webbRelayer.substrateMixerWithdraw({
        chain: aliceNode.name,
        id: withdrawalProof.id,
        proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
        root: Array.from(hexToU8a(invalidRootBytes)),
        nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
        refund: withdrawalProof.refund,
        fee: withdrawalProof.fee,
        recipient: withdrawalProof.recipient,
        relayer: withdrawalProof.relayer,
      });
    } catch (e) {
      // Expect an error to be thrown
      expect(e).to.not.be.null;
      // Runtime Error that indicates invalid withdrawal proof
      expect(e).to.contain(
          'Runtime error: RuntimeError(Module { index: 40, error: 4 }'
      );
    }
  });

  it('Should fail to withdraw if recipient address is invalid', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Dave');
    const note = await makeDeposit(api, aliceNode, account);
    const withdrawalProof = await initWithdrawal(
        api,
        webbRelayer,
        account,
        note
    );

    const invalidAddress = '5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PTXFy';

    // now we need to submit the withdrawal transaction.
    try {
      // try to withdraw with invalid address
      await webbRelayer.substrateMixerWithdraw({
        chain: aliceNode.name,
        id: withdrawalProof.id,
        proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
        root: Array.from(hexToU8a(withdrawalProof.root)),
        nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
        refund: withdrawalProof.refund,
        fee: withdrawalProof.fee,
        recipient: withdrawalProof.recipient,
        relayer: invalidAddress,
      });
    } catch (e) {
      // Expect an error to be thrown
      expect(e).to.not.be.null;
      // Runtime Error that indicates invalid withdrawal proof
      expect(e).to.contain(
          'Runtime error: RuntimeError(Module { index: 40, error: 1 }'
      );
    }
  });

  it('Should fail to withdraw with invalid nullifier hash', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Eve');
    const note = await makeDeposit(api, aliceNode, account);
    const withdrawalProof = await initWithdrawal(
        api,
        webbRelayer,
        account,
        note
    );

    const nullifierHash = hexToU8a(withdrawalProof.root);
    // flip a bit in the proof, so it is invalid
    const flipCount = nullifierHash.length / 8;
    for (let i = 0; i < flipCount; i++) {
      nullifierHash[i] |= 0x42;
    }
    const invalidNullifierHash = u8aToHex(nullifierHash);
    expect(withdrawalProof.proofBytes).to.not.eq(invalidNullifierHash);

    // now we need to submit the withdrawal transaction.
    try {
      // try to withdraw with invalid address
      await webbRelayer.substrateMixerWithdraw({
        chain: aliceNode.name,
        id: withdrawalProof.id,
        proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
        root: Array.from(hexToU8a(withdrawalProof.root)),
        nullifierHash: Array.from(hexToU8a(invalidNullifierHash)),
        refund: withdrawalProof.refund,
        fee: withdrawalProof.fee,
        recipient: withdrawalProof.recipient,
        relayer: withdrawalProof.relayer,
      });
    } catch (e) {
      // Expect an error to be thrown
      expect(e).to.not.be.null;
      // Runtime Error that indicates invalid withdrawal proof
      expect(e).to.contain(
          'Runtime error: RuntimeError(Module { index: 40, error: 1 }'
      );
    }
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
  fee?: number;
  refund?: number;
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
      fee: opts.fee === undefined ? 0 : opts.fee,
      refund: opts.refund === undefined ? 0 : opts.refund,
      provingKey,
    };
    const zkProof = await pm.proof(proofInput);

    console.log(`DBG: Root VALUE From ZK mixer is ${JSON.stringify(zkProof.root)}` );

    return {
      id: treeId,
      proofBytes: `0x${zkProof.proof}`,
      root: `0x${zkProof.root}`,
      nullifierHash: `0x${zkProof.nullifierHash}`,
      recipient: opts.recipient,
      relayer: opts.relayer,
      fee: opts.fee === undefined ? 0 : opts.fee,
      refund: opts.refund === undefined ? 0 : opts.refund,
    };
  } catch (error) {
    //@ts-ignore
    console.error(error.error_message);
    //@ts-ignore
    console.error(error.code);
    throw error;
  }
}

function createAccount(accountId: string): any {
  const keyring = new Keyring({ type: 'sr25519' });
  const account = keyring.addFromUri(accountId);

  return account;
}

async function makeDeposit(
  api: any,
  aliceNode: any,
  account: any
): Promise<any> {
  const { tx, note } = await createMixerDepositTx(api);

  // send the deposit transaction.
  const txSigned = await tx.signAsync(account);
  await aliceNode.executeTransaction(txSigned);

  return note;
}

async function initWithdrawal(
  api: any,
  webbRelayer: any,
  account: any,
  note: any
): Promise<WithdrawalProof> {
  // next we need to prepare the withdrawal transaction.
  // create correct proof with right address
  const withdrawalProof = await createMixerWithdrawProof(api, note, {
    recipient: account.address,
    relayer: account.address,
  });
  // ping the relayer!
  await webbRelayer.ping();

  return withdrawalProof;
}
