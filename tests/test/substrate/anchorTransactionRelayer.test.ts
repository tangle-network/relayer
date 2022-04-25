/*
 * Copyright 2022 Webb Technologies Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
// This our basic Substrate Anchor Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just to relay transactions for us.

import { expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import isCi from 'is-ci';
import child from 'child_process';
import { sleep } from '../../lib/sleep.js';
import { WebbRelayer, Pallet } from '../../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../../lib/localProtocolSubstrate.js';
import {
  UsageMode,
  defaultEventsWatcherValue,
} from '../../lib/substrateNodeBase.js';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { decodeAddress } from '@polkadot/util-crypto';
import { ethAddressFromString } from '../utils/util.js';
import {
  Note,
  NoteGenInput,
  ProvingManagerSetupInput,
  ProvingManagerWrapper,
} from '@webb-tools/sdk-core';

describe('Substrate Anchor Transaction Relayer', function () {
  this.timeout(250000);
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
    const enabledPallets: Pallet[] = [
      {
        pallet: 'AnchorBn254',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];
    aliceNode = await LocalProtocolSubstrate.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enabledPallets,
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
    // wait for protocol substrate node to get started
    await sleep(5000);
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

  it('number of deposits made should be equal to number of leaves in cache', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Dave');
    // Make multiple deposits
    const noOfDeposit = 3;
    for (let i = 0, len = noOfDeposit; i < len; i++) {
      const note = await makeDeposit(api, aliceNode, account);
    }
    // now we wait for all deposit to be saved in LeafStorageCache
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: (noOfDeposit - 1).toString(),
      },
    });
    // chainId
    const chainId = 1080;
    const chainIdHex = chainId.toString(16);
    //@ts-ignore
    const treeIds = await api.query.anchorBn254.anchors?.keys();
    //@ts-ignore
    const sorted = treeIds?.map(id => Number(id.toHuman()[0])).sort();
    //@ts-ignore
    const treeId = sorted[0] || 5;
    // Since substrate pallet does not have address, we use treeId
    // converted treeId to H160 ethereum type address
    const nodeName = ethAddressFromString(treeId.toString());
    // now we call relayer leaf API to check no of leaves stored in LeafStorageCache
    // are equal to no of deposits made.
    const response = await webbRelayer.getLeaves(chainIdHex, nodeName);
    expect(noOfDeposit).to.equal(response.leaves.length);
  });

  it('Simple Anchor Transaction', async () => {
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
    console.log(`balance before withdrawal is ${balance.free.toBigInt()}`);

    const roots = [
      Array.from(
        hexToU8a(
          '0x0000000000000000000000000000000000000000000000000000000000000000'
        )
      ),
      Array.from(hexToU8a(withdrawalProof.root)),
    ];

    // now we need to submit the withdrawal transaction.
    const txHash = await webbRelayer.substrateAnchorWithdraw({
      chain: aliceNode.name,
      id: withdrawalProof.id,
      proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
      roots: roots,
      nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
      refund: withdrawalProof.refund,
      fee: withdrawalProof.fee,
      recipient: withdrawalProof.recipient,
      relayer: withdrawalProof.relayer,
      refreshCommitment: Array.from(
        hexToU8a(withdrawalProof.refreshCommitment)
      ),
      extDataHash: Array.from(
        hexToU8a(
          '0x0000000000000000000000000000000000000000000000000000000000000000'
        )
      ),
    });

    expect(txHash).to.be.not.null;

    // get the balance after withdrawal is done and see if it increases
    // @ts-ignore
    const { nonce: nonceAfter, data: balanceAfter } = await api.query.system!
      .account!(withdrawalProof.recipient);
    let balanceAfterWithdraw = balanceAfter.free.toBigInt();
    console.log(`balance after withdrawal is ${balanceAfter.free.toBigInt()}`);
    expect(balanceAfterWithdraw > initialBalance);
  });

  it('Should fail to withdraw if address is invalid', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Dave');
    const note = await makeDeposit(api, aliceNode, account);
    const withdrawalProof = await initWithdrawal(
      api,
      webbRelayer,
      account,
      note
    );

    const roots = [
      Array.from(
        hexToU8a(
          '0x0000000000000000000000000000000000000000000000000000000000000000'
        )
      ),
      Array.from(hexToU8a(withdrawalProof.root)),
    ];

    const invalidAddress = '5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PTXFy';

    // now we need to submit the withdrawal transaction.
    try {
      // try to withdraw with invalid address
      await webbRelayer.substrateAnchorWithdraw({
        chain: aliceNode.name,
        id: withdrawalProof.id,
        proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
        roots: roots,
        nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
        refund: withdrawalProof.refund,
        fee: withdrawalProof.fee,
        recipient: invalidAddress,
        relayer: invalidAddress,
        refreshCommitment: Array.from(
          hexToU8a(withdrawalProof.refreshCommitment)
        ),
        extDataHash: Array.from(
          hexToU8a(
            '0x0000000000000000000000000000000000000000000000000000000000000000'
          )
        ),
      });
    } catch (e) {
      console.log(`error is ${e}`);

      // Expect an error to be thrown
      expect(e).to.not.be.null;
      // Runtime Error that indicates invalid withdrawal proof
      expect(e).to.contain(
        'Runtime error: RuntimeError(Module { index: 41, error: 2 }'
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
    proofBytes[1] |= 0x42;
    const invalidProofBytes = u8aToHex(proofBytes);
    expect(withdrawalProof.proofBytes).to.not.eq(invalidProofBytes);

    const roots = [
      Array.from(
        hexToU8a(
          '0x0000000000000000000000000000000000000000000000000000000000000000'
        )
      ),
      Array.from(hexToU8a(withdrawalProof.root)),
    ];

    // now we need to submit the withdrawal transaction.
    try {
      // try to withdraw with invalid address
      // try to withdraw with invalid address
      await webbRelayer.substrateAnchorWithdraw({
        chain: aliceNode.name,
        id: withdrawalProof.id,
        proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
        roots: roots,
        nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
        refund: withdrawalProof.refund,
        fee: withdrawalProof.fee,
        recipient: withdrawalProof.recipient,
        relayer: withdrawalProof.relayer,
        refreshCommitment: Array.from(
          hexToU8a(withdrawalProof.refreshCommitment)
        ),
        extDataHash: Array.from(
          hexToU8a(
            '0x0000000000000000000000000000000000000000000000000000000000000000'
          )
        ),
      });
    } catch (e) {
      console.log(`error is ${e}`);

      // Expect an error to be thrown
      expect(e).to.not.be.null;
      // Runtime Error that indicates invalid withdrawal proof
      expect(e).to.contain(
        'Runtime error: RuntimeError(Module { index: 41, error: 2 }'
      );
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

    const roots = [
      Array.from(
        hexToU8a(
          '0x0000000000000000000000000000000000000000000000000000000000000000'
        )
      ),
      Array.from(hexToU8a(withdrawalProof.root)),
    ];

    // now we need to submit the withdrawal transaction.
    try {
      // try to withdraw with invalid address
      await webbRelayer.substrateAnchorWithdraw({
        chain: aliceNode.name,
        id: withdrawalProof.id,
        proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
        roots: roots,
        nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
        refund: withdrawalProof.refund,
        fee: invalidFee,
        recipient: withdrawalProof.recipient,
        relayer: withdrawalProof.relayer,
        refreshCommitment: Array.from(
          hexToU8a(withdrawalProof.refreshCommitment)
        ),
        extDataHash: Array.from(
          hexToU8a(
            '0x0000000000000000000000000000000000000000000000000000000000000000'
          )
        ),
      });
    } catch (e) {
      console.log(`error is ${e}`);

      // Expect an error to be thrown
      expect(e).to.not.be.null;
      // Runtime Error that indicates invalid withdrawal proof
      expect(e).to.contain(
        'Runtime error: RuntimeError(Module { index: 41, error: 2 }'
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

async function createAnchorDepositTx(api: ApiPromise): Promise<{
  tx: SubmittableExtrinsic<'promise'>;
  note: Note;
}> {
  const noteInput: NoteGenInput = {
    protocol: 'anchor',
    version: 'v2',
    sourceChain: '2199023256632',
    targetChain: '2199023256632',
    sourceIdentifyingData: '3',
    targetIdentifyingData: '3',
    tokenSymbol: 'WEBB',
    amount: '1',
    denomination: '18',
    backend: 'Arkworks',
    hashFunction: 'Poseidon',
    curve: 'Bn254',
    width: '4',
    exponentiation: '5',
  };
  const note = await Note.generateNote(noteInput);
  //@ts-ignore
  const treeIds = await api.query.anchorBn254.anchors?.keys();
  //@ts-ignore
  const sorted = treeIds?.map(id => Number(id.toHuman()[0])).sort();
  //@ts-ignore
  const treeId = sorted[0] || 5;
  const leaf = note.getLeaf();
  const tx = api.tx.anchorBn254!.deposit!(treeId, leaf);
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
  nullifierHash: string;
  recipient: string;
  relayer: string;
  fee: number;
  refund: number;
  refreshCommitment: string;
  root: string;
};

async function createAnchorWithdrawProof(
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
    //@ts-ignore
    const treeIds = await api.query.anchorBn254.anchors?.keys();
    //@ts-ignore
    const sorted = treeIds?.map(id => Number(id.toHuman()[0])).sort();
    //@ts-ignore
    const treeId = sorted[0] || 5;
    //@ts-ignore
    const getLeaves = api.rpc.mt.getLeaves;
    const treeLeaves: Uint8Array[] = await getLeaves(treeId, 0, 511);

    // Get tree root on chain
    // @ts-ignore
    const treeRoot = await api.query.merkleTreeBn254.trees(treeId);

    const pm = new ProvingManagerWrapper('direct-call');
    const leafHex = u8aToHex(note.getLeaf());

    const leafIndex = treeLeaves.findIndex((l) => u8aToHex(l) === leafHex);
    expect(leafIndex).to.be.greaterThan(-1);
    const gitRoot = child
      .execSync('git rev-parse --show-toplevel')
      .toString()
      .trim();

    // make a root set from the tree root
    // @ts-ignore
    const rootValue = treeRoot.toHuman() as { root: string };
    const treeRootArray = [
      hexToU8a(
        '0x0000000000000000000000000000000000000000000000000000000000000000'
      ),
      hexToU8a(rootValue.root),
    ];

    const provingKeyPath = path.join(
      gitRoot,
      'tests',
      'protocol-substrate-fixtures',
      'fixed-anchor',
      'bn254',
      'x5',
      '2',
      'proving_key_uncompressed.bin'
    );
    const provingKey = fs.readFileSync(provingKeyPath);

    // @ts-ignore
    const proofInput: ProvingManagerSetupInput = {
      note: note.serialize(),
      relayer: relayerAddressHex,
      recipient: recipientAddressHex,
      leaves: treeLeaves,
      leafIndex,
      fee: opts.fee === undefined ? 0 : opts.fee,
      refund: opts.refund === undefined ? 0 : opts.refund,
      provingKey,
      roots: treeRootArray,
      refreshCommitment:
        '0000000000000000000000000000000000000000000000000000000000000000',
    };

    const zkProof = await pm.proof(proofInput);
    return {
      id: treeId,
      proofBytes: `0x${zkProof.proof}`,
      nullifierHash: `0x${zkProof.nullifierHash}`,
      recipient: opts.recipient,
      relayer: opts.relayer,
      fee: opts.fee === undefined ? 0 : opts.fee,
      refund: opts.refund === undefined ? 0 : opts.refund,
      refreshCommitment:
        '0x0000000000000000000000000000000000000000000000000000000000000000',
      // @ts-ignore
      root: treeRoot.toHuman().root,
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
  const { tx, note } = await createAnchorDepositTx(api);

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
  const withdrawalProof = await createAnchorWithdrawProof(api, note, {
    recipient: account.address,
    relayer: account.address,
  });
  // ping the relayer!
  await webbRelayer.ping();

  return withdrawalProof;
}

