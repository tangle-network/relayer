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
// This our basic Substrate VAnchor Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just to relay transactions for us.

import { assert, expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import isCi from 'is-ci';
import child from 'child_process';
import {
  WebbRelayer,
  Pallet,
  SubstrateVAnchorExtData,
  SubstrateVAnchorProofData,
  SubstrateFeeInfo,
} from '../../lib/webbRelayer.js';

import { BigNumber, ethers } from 'ethers';
import { ApiPromise } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import { naclEncrypt, randomAsU8a } from '@polkadot/util-crypto';

import {
  ProvingManagerSetupInput,
  ArkworksProvingManager,
  Utxo,
  calculateTypedChainId,
  ChainType,
} from '@webb-tools/sdk-core';
import { currencyToUnitI128, UsageMode } from '@webb-tools/test-utils';
import {
  createAccount,
  defaultEventsWatcherValue,
  generateVAnchorNote,
} from '../../lib/utils.js';
import { LocalTangle } from '../../lib/localTangle.js';
import { formatEther } from 'ethers/lib/utils.js';

describe('Substrate VAnchor Private Transaction Relayer Tests', function () {
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalTangle;
  let charlieNode: LocalTangle;
  let webbRelayer: WebbRelayer;
  const PK1 = u8aToHex(ethers.utils.randomBytes(32));

  before(async () => {
    const usageMode: UsageMode = isCi
      ? { mode: 'docker', forcePullImage: false }
      : {
          mode: 'host',
          nodePath: path.resolve(
            '../../tangle/target/release/tangle-standalone'
          ),
        };
    const enabledPallets: Pallet[] = [
      {
        pallet: 'VAnchorBn254',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];

    aliceNode = await LocalTangle.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });

    charlieNode = await LocalTangle.start({
      name: 'dkg-charlie',
      authority: 'charlie',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });
    // Wait until we are ready and connected
    const api = await aliceNode.api();
    await api.isReady;

    const chainId = await aliceNode.getChainId();

    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      chainId: chainId,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
      enabledPallets,
    });

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(8000, 8888) });
    webbRelayer = new WebbRelayer({
      commonConfig: {
        port: relayerPort,
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
    });
    await webbRelayer.waitUntilReady();
  });

  it.only('should withdraw using private transaction ', async () => {
    console.log("start");
    const api = await aliceNode.api();
    // 1. Create vanchor on Substrate chain with height 30 and maxEdges = 1
    const createVAnchorCall = api.tx.vAnchorBn254.create(1, 30, 0);
    // execute sudo transaction.
    await aliceNode.sudoExecuteTransaction(createVAnchorCall);
    const nextTreeId = await api.query.merkleTreeBn254.nextTreeId();
    const treeId = nextTreeId.toNumber() - 1;

    // ChainId of the substrate chain
    const substrateChainId = await aliceNode.getChainId();
    const typedSourceChainId = calculateTypedChainId(
      ChainType.Substrate,
      substrateChainId
    );

    // 2. Deposit amount on substrate chain.
    console.log("deposit 1");
    const data = await vanchorDeposit(
      typedSourceChainId.toString(), // source chain Id
      typedSourceChainId.toString(), // target chain Id
      1_0000_000, // public amount
      treeId,
      api,
      aliceNode
    );
    console.log('Deposit made');
    // now we wait for all deposit to be saved in LeafStorageCache.
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: 2,
      },
    });

    // 3. Now we withdraw it on bob's account using private transaction.
    const account = createAccount('//Bob');
    // Bob's balance after withdrawal
    const bobBalanceBefore = await api.query.system.account(account.address);

    // get refund amount
    const feeInfoResponse1 = await webbRelayer.getSubstrateFeeInfo(
      substrateChainId,
      0
    );
    expect(feeInfoResponse1.status).equal(200);
    const feeInfo1 =
      await (feeInfoResponse1.json() as Promise<SubstrateFeeInfo>);
    console.log('fee info 1: ', feeInfo1);

    const vanchorData = await vanchorWithdraw(
      typedSourceChainId.toString(),
      typedSourceChainId.toString(),
      account.address,
      data.depositUtxos,
      treeId,
      BigNumber.from(feeInfo1.estimatedFee),
      // TODO: need to convert this once there is exchange rate between native
      //       token and wrapped token
      BigNumber.from(feeInfo1.maxRefund),
      api
    );
    // Now we construct payload for substrate private transaction.
    // Convert [u8;4] to u32 asset Id
    const token = new DataView(vanchorData.extData.token.buffer, 0);
    const substrateExtData: SubstrateVAnchorExtData = {
      recipient: vanchorData.extData.recipient,
      relayer: vanchorData.extData.relayer,
      extAmount: vanchorData.extData.extAmount.toString(),
      fee: vanchorData.extData.fee.toString(),
      encryptedOutput1: Array.from(
        hexToU8a(vanchorData.extData.encryptedOutput1)
      ),
      encryptedOutput2: Array.from(
        hexToU8a(vanchorData.extData.encryptedOutput2)
      ),
      refund: vanchorData.extData.refund.toString(),
      token: token.getUint32(0, true),
    };

    const substrateProofData: SubstrateVAnchorProofData = {
      proof: Array.from(hexToU8a(vanchorData.proofData.proof)),
      extDataHash: Array.from(vanchorData.proofData.extDataHash),
      extensionRoots: vanchorData.proofData.roots.map((root) =>
        Array.from(root)
      ),
      publicAmount: Array.from(vanchorData.proofData.publicAmount),
      roots: vanchorData.proofData.roots.map((root) => Array.from(root)),
      outputCommitments: vanchorData.proofData.outputCommitments.map((com) =>
        Array.from(com)
      ),
      inputNullifiers: vanchorData.proofData.inputNullifiers.map((com) =>
        Array.from(hexToU8a(com))
      ),
    };

    console.log(substrateExtData);
    const info = await api.tx.vAnchorBn254.transact(treeId, substrateProofData,
      substrateExtData).paymentInfo(account);
    console.log(info);
    const partialFee = 10958835753;
    const feeInfoResponse2 = await webbRelayer.getSubstrateFeeInfo(
      substrateChainId,
      partialFee
    );
    expect(feeInfoResponse2.status).equal(200);
    const feeInfo2 =
      await (feeInfoResponse2.json() as Promise<SubstrateFeeInfo>);
    console.log(feeInfo2);

    // now we withdraw using private transaction
    await webbRelayer.substrateVAnchorWithdraw(
      substrateChainId,
      treeId,
      substrateExtData,
      substrateProofData
    );

    // now we wait for relayer to execute private transaction.
    await webbRelayer.waitForEvent({
      kind: 'private_tx',
      event: {
        ty: 'SUBSTRATE',
        chain_id: substrateChainId.toString(),
        finalized: true,
      },
    });

    // Bob's balance after withdrawal.
    const BobBalanceAfter = await api.query.system.account(account.address);
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    //@ts-ignore
    console.log('balance after : ', BobBalanceAfter.data.free);
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    //@ts-ignore
    assert(BobBalanceAfter.data.free > bobBalanceBefore.data.free);
  });

  after(async () => {
    await aliceNode?.stop();
    await charlieNode?.stop();
    await webbRelayer?.stop();
  });
});

// Helper methods, we can move them somewhere if we end up using them again.

async function vanchorWithdraw(
  typedTargetChainId: string,
  typedSourceChainId: string,
  recipient: string,
  depositUtxos: [Utxo, Utxo],
  treeId: number,
  fee: BigNumber,
  refund: BigNumber,
  api: ApiPromise
): Promise<{ extData: any; proofData: any }> {
  const secret = randomAsU8a();
  const gitRoot = child
    .execSync('git rev-parse --show-toplevel')
    .toString()
    .trim();

  const pkPath = path.join(
    // tests path
    gitRoot,
    'tests',
    'substrate-fixtures',
    'vanchor',
    'bn254',
    'x5',
    '2-2-2',
    'proving_key_uncompressed.bin'
  );
  const pk_hex = fs.readFileSync(pkPath).toString('hex');
  const pk = hexToU8a(pk_hex);
  const leavesMap = {};
  // get source chain (evm) leaves.
  const substrateLeaves = await api.derive.merkleTreeBn254.getLeavesForTree(
    treeId,
    0,
    1
  );
  assert(substrateLeaves.length === 2, 'Invalid substrate leaves length');
  substrateLeaves.map((leaf, index) => {
    if (depositUtxos[0].commitment.toString() === leaf.toString()) {
      depositUtxos[0].setIndex(index);
    } else if (depositUtxos[1].commitment.toString() === leaf.toString()) {
      depositUtxos[1].setIndex(index);
    }
  });
  leavesMap[typedSourceChainId.toString()] = substrateLeaves;

  // Output UTXOs configs
  const output1 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: '0',
    chainId: typedSourceChainId,
  });
  const output2 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: '0',
    chainId: typedSourceChainId,
  });

  // Configure a new proving manager with direct call
  const provingManager = new ArkworksProvingManager(null);
  const assetId = new Uint8Array([0, 0, 0, 0]); // WEBB native token asset Id.

  const address = recipient;
  console.log('fee: ', formatEther(fee));
  console.log('refund: ', formatEther(refund));
  const fee2 = fee.add(refund);
  console.log('fee2: ', formatEther(fee2));
  console.log('fee2          : ', fee2.toString());
  const withdrawAmount = depositUtxos.reduce((acc, utxo) => {
    return BigNumber.from(utxo.amount).add(acc);
  }, BigNumber.from(0));
  console.log('withdrawAmount: ', formatEther(withdrawAmount));
  const extAmount = withdrawAmount.mul(-1);
  console.log('extAmount: ', formatEther(extAmount));

  const publicAmount = -withdrawAmount;

  const tree = await api.query.merkleTreeBn254.trees(treeId);
  const root = tree.unwrap().root.toHex();
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore TODO: Fix the issue with rpc type augmentation
  const neighborRoots: string[] = await api.rpc.lt
    .getNeighborRoots(treeId)
    .then((roots) => roots.toHuman());
  const rootsSet = [hexToU8a(root), hexToU8a(neighborRoots[0])];
  const decodedAddress = decodeAddress(address);
  const { encrypted: comEnc1 } = naclEncrypt(output1.commitment, secret);
  const { encrypted: comEnc2 } = naclEncrypt(output2.commitment, secret);

  const setup: ProvingManagerSetupInput<'vanchor'> = {
    chainId: typedTargetChainId.toString(),
    leafIds: depositUtxos.map((utxo) => {
      return {
        index: utxo.index!,
        typedChainId: Number(utxo.chainId),
      };
    }),
    inputUtxos: depositUtxos,
    leavesMap: leavesMap,
    output: [output1, output2],
    encryptedCommitments: [comEnc1, comEnc2],
    provingKey: pk,
    publicAmount: String(publicAmount),
    roots: rootsSet,
    relayer: decodedAddress,
    recipient: decodedAddress,
    extAmount: extAmount.toString(),
    fee: fee2.toString(),
    refund: String(refund),
    token: assetId,
  };

  const data = await provingManager.prove('vanchor', setup);
  const extData = {
    relayer: address,
    recipient: address,
    fee: fee2,
    refund: String(refund),
    token: assetId,
    extAmount: extAmount,
    encryptedOutput1: u8aToHex(comEnc1),
    encryptedOutput2: u8aToHex(comEnc2),
  };
  const proofData = {
    proof: `0x${data.proof}`,
    publicAmount: data.publicAmount,
    roots: rootsSet,
    inputNullifiers: data.inputUtxos.map((input) => `0x${input.nullifier}`),
    outputCommitments: data.outputUtxos.map((utxo) => utxo.commitment),
    extDataHash: data.extDataHash,
  };

  return { extData, proofData };
}

async function vanchorDeposit(
  typedTargetChainId: string,
  typedSourceChainId: string,
  publicAmountUint: number,
  treeId: number,
  api: ApiPromise,
  aliceNode: LocalTangle
): Promise<{ depositUtxos: [Utxo, Utxo] }> {
  const account = createAccount('//Dave');
  const secret = randomAsU8a();
  const gitRoot = child
    .execSync('git rev-parse --show-toplevel')
    .toString()
    .trim();

  const pkPath = path.join(
    // tests path
    gitRoot,
    'tests',
    'substrate-fixtures',
    'vanchor',
    'bn254',
    'x5',
    '2-2-2',
    'proving_key_uncompressed.bin'
  );
  const pk_hex = fs.readFileSync(pkPath).toString('hex');
  const pk = hexToU8a(pk_hex);

  const vkPath = path.join(
    // tests path
    gitRoot,
    'tests',
    'substrate-fixtures',
    'vanchor',
    'bn254',
    'x5',
    '2-2-2',
    'verifying_key_uncompressed.bin'
  );
  const vk_hex = fs.readFileSync(vkPath).toString('hex');
  const vk = hexToU8a(vk_hex);

  // dummy Deposit Note. Input note is directed toward source chain
  const depositNote = await generateVAnchorNote(
    0,
    Number(typedSourceChainId),
    Number(typedSourceChainId),
    0
  );

  const note1 = depositNote;
  const note2 = await note1.getDefaultUtxoNote();
  const publicAmount = currencyToUnitI128(publicAmountUint);
  console.log('deposit amount: ', publicAmount.toString());
  const notes = [note1, note2];
  // Output UTXOs configs
  const output1 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: publicAmount.toString(),
    chainId: typedTargetChainId,
  });
  const output2 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: '0',
    chainId: typedTargetChainId,
  });

  // Configure a new proving manager with direct call
  const provingManager = new ArkworksProvingManager(null);
  const leavesMap = {};

  const address = account.address;
  const extAmount = currencyToUnitI128(publicAmountUint);
  const fee = 0;
  const refund = 0;
  // Initially leaves will be empty
  leavesMap[typedTargetChainId.toString()] = [];
  const tree = await api.query.merkleTreeBn254.trees(treeId);
  const root = tree.unwrap().root.toHex();
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  const neighborRoots: string[] = await api.rpc.lt
    .getNeighborRoots(treeId)
    .then((roots) => roots.toHuman());

  const rootsSet = [hexToU8a(root), hexToU8a(neighborRoots[0])];
  const decodedAddress = decodeAddress(address);
  const assetId = new Uint8Array([0, 0, 0, 0]); // WEBB native token asset Id.
  const { encrypted: comEnc1 } = naclEncrypt(output1.commitment, secret);
  const { encrypted: comEnc2 } = naclEncrypt(output2.commitment, secret);
  const LeafId = {
    index: 0,
    typedChainId: Number(typedSourceChainId),
  };
  const setup: ProvingManagerSetupInput<'vanchor'> = {
    chainId: typedSourceChainId.toString(),
    inputUtxos: notes.map((n) => new Utxo(n.note.getUtxo())),
    leafIds: [LeafId, LeafId],
    leavesMap: leavesMap,
    output: [output1, output2],
    encryptedCommitments: [comEnc1, comEnc2],
    provingKey: pk,
    publicAmount: String(publicAmount),
    roots: rootsSet,
    relayer: decodedAddress,
    recipient: decodedAddress,
    extAmount: extAmount.toString(),
    fee: fee.toString(),
    refund: String(refund),
    token: assetId,
  };

  const data = await provingManager.prove('vanchor', setup);
  const extData = {
    relayer: address,
    recipient: address,
    fee,
    refund: String(refund),
    token: assetId,
    extAmount: extAmount.toString(),
    encryptedOutput1: u8aToHex(comEnc1),
    encryptedOutput2: u8aToHex(comEnc2),
  };

  const vanchorProofData = {
    proof: `0x${data.proof}`,
    publicAmount: data.publicAmount,
    roots: rootsSet,
    inputNullifiers: data.inputUtxos.map((input) => `0x${input.nullifier}`),
    outputCommitments: data.outputUtxos.map((utxo) => utxo.commitment),
    extDataHash: data.extDataHash,
  };

  // now we call the vanchor transact to deposit on substrate.
  const transactCall = api.tx.vAnchorBn254.transact(
    treeId,
    vanchorProofData,
    extData
  );
  const txSigned = await transactCall.signAsync(account);
  await aliceNode.executeTransaction(txSigned);
  return { depositUtxos: data.outputUtxos };
}
