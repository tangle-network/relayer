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
// This our basic Substrate VAnchor Transaction Relayer Tests (Circom).
// These are for testing the basic relayer functionality. which is just to relay transactions for us.

import { assert, expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import isCi from 'is-ci';
import child from 'child_process';
import { WebbRelayer, Pallet } from '../../lib/webbRelayer.js';

import { BigNumber, ethers } from 'ethers';
import { ApiPromise } from '@polkadot/api';
import { u8aToHex, hexToU8a, BN } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import { naclEncrypt, randomAsU8a } from '@polkadot/util-crypto';

import {
  ProvingManagerSetupInput,
  ArkworksProvingManager,
  Utxo,
  calculateTypedChainId,
  ChainType,
  Keypair,
  CircomUtxo,
  randomBN,
  toFixedHex,
  CircomProvingManager,
  LeafIdentifier,
} from '@webb-tools/sdk-core';
import { currencyToUnitI128, UsageMode } from '@webb-tools/test-utils';
import {
  createAccount,
  defaultEventsWatcherValue,
  generateVAnchorNote,
} from '../../lib/utils.js';
import { LocalTangle } from '../../lib/localTangle.js';
import { verify_js_proof } from '@webb-tools/wasm-utils/njs/wasm-utils-njs.js';
import { fetchComponentsFromFilePaths } from '@webb-tools/utils';

describe.only('Substrate VAnchor Private Transaction Relayer Tests Using Circom', function() {
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalTangle;
  let bobNode: LocalTangle;
  let charlieNode: LocalTangle;
  let webbRelayer: WebbRelayer;
  const PK1 = u8aToHex(ethers.utils.randomBytes(32));

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

    bobNode = await LocalTangle.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });

    charlieNode = await LocalTangle.start({
      name: 'substrate-charlie',
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

  it('should withdraw using private transaction ', async () => {
    const api = await aliceNode.api();
    // 1. Create vanchor on Substrate chain with height 30 and maxEdges = 1
    const createVAnchorCall = api.tx.vAnchorCircomBn254!.create!(1, 30, 0);
    // execute sudo transaction.
    await aliceNode.sudoExecuteTransaction(createVAnchorCall);
    const nextTreeId = await api.query.merkleTreeCircomBn254!.nextTreeId!();
    const treeId = Number(nextTreeId.toHuman()) - 1;

    // ChainId of the substrate chain
    const substrateChainId = await aliceNode.getChainId();
    const typedSourceChainId = calculateTypedChainId(
      ChainType.Substrate,
      substrateChainId
    );

    // 2. Deposit amount on substrate chain.
    const data = await vanchorDeposit(
      typedSourceChainId.toString(), // source chain Id
      typedSourceChainId.toString(), // target chain Id
      1000000000, // public amount
      treeId,
      api,
      aliceNode
    );
    console.log('data', data);
    // now we wait for all deposit to be saved in LeafStorageCache.
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: 2,
      },
    });

    // Withdraw Flow
    /*
    // 3. Now we withdraw it on bob's account using private transaction.
    const account = createAccount('//Bob');
    // Bob's balance after withdrawal
    const bobBalanceBefore = await api.query.system.account(account.address);

    const dummyVanchorData = await vanchorWithdraw(
      typedSourceChainId.toString(),
      typedSourceChainId.toString(),
      account.address,
      data.depositUtxos,
      treeId,
      BigInt(0),
      // TODO: need to convert this once there is exchange rate between native
      //       token and wrapped token
      BigInt(0),
      api
    );
    const token = new DataView(dummyVanchorData.extData.token.buffer, 0);

    const info = await api.tx.vAnchorBn254
      .transact(treeId, dummyVanchorData.proofData, dummyVanchorData.extData)
      .paymentInfo(account);
    const feeInfoResponse2 = await webbRelayer.getSubstrateFeeInfo(
      substrateChainId,
      info.partialFee.toBn()
    );
    expect(feeInfoResponse2.status).equal(200);
    const feeInfo2 =
      await (feeInfoResponse2.json() as Promise<SubstrateFeeInfo>);
    const estimatedFee = BigInt(feeInfo2.estimatedFee);

    const refund = BigInt(0);
    const feeTotal = estimatedFee + refund;
    const vanchorData = await vanchorWithdraw(
      typedSourceChainId.toString(),
      typedSourceChainId.toString(),
      account.address,
      data.depositUtxos,
      treeId,
      feeTotal,
      // TODO: need to convert this once there is exchange rate between native
      //       token and wrapped token
      refund,
      api
    );
    const substrateExtData: SubstrateVAnchorExtData = {
      recipient: vanchorData.extData.recipient,
      relayer: vanchorData.extData.relayer,
      extAmount: BigNumber.from(vanchorData.extData.extAmount)
        .toHexString()
        .replace('0x', ''),
      fee: BigNumber.from(feeTotal.toString()).toHexString(),
      encryptedOutput1: Array.from(
        hexToU8a(vanchorData.extData.encryptedOutput1)
      ),
      encryptedOutput2: Array.from(
        hexToU8a(vanchorData.extData.encryptedOutput2)
      ),
      refund: BigNumber.from(refund.toString()).toHexString(),
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
    */
  });

  after(async () => {
    await aliceNode?.stop();
    await bobNode?.stop();
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
  fee: bigint,
  refund: bigint,
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
  const withdrawAmount = depositUtxos.reduce((acc, utxo) => {
    return BigInt(utxo.amount) + acc;
  }, BigInt(0));

  const publicAmount = -withdrawAmount;
  const extAmount = -(withdrawAmount - fee);

  console.log({
    fee: fee.toString(),
    extAmount: extAmount.toString(),
    publicAmount: publicAmount.toString(),
  });

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
    publicAmount: publicAmount.toString(),
    roots: rootsSet,
    relayer: decodedAddress,
    recipient: decodedAddress,
    extAmount: extAmount.toString(),
    fee: fee.toString(),
    refund: refund.toString(),
    token: assetId,
  };

  const data = await provingManager.prove('vanchor', setup);

  const isValidProof = verify_js_proof(
    data.proof,
    data.publicInputs,
    u8aToHex(vk).replace('0x', ''),
    'Bn254'
  );
  console.log('Is proof valid : ', isValidProof);
  expect(isValidProof).to.be.true;

  const extData = {
    relayer: address,
    recipient: address,
    fee: fee.toString(),
    refund: refund.toString(),
    token: assetId,
    extAmount: extAmount.toString(),
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

  const inputUtxos = notes.map((n) => new Utxo(n.note.getUtxo()));
  const outputUtxos: [Utxo, Utxo] = [output1, output2];

  const leavesMap = {};

  const address = account.address;
  const assetId = new Uint8Array([0, 0, 0, 0]); // WEBB native token asset Id.
  const fee = BigNumber.from(0);
  const refund = BigNumber.from(0);
  // Initially leaves will be empty
  leavesMap[typedTargetChainId.toString()] = [];
  const tree = await api.query.merkleTreeCircomBn254!.trees!(treeId);
  //@ts-ignore
  const root = tree.unwrap().root.toHex();
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  const neighborRoots: string[] = await api.rpc.lt
    .getNeighborRoots(treeId)
    .then((roots) => roots.toHuman());

  const rootsSet: string[] = [root, neighborRoots[0]!];
  const decodedAddress = u8aToHex(decodeAddress(address));

  const data = await setupTransaction(
    Number(typedSourceChainId),
    inputUtxos,
    outputUtxos,
    fee,
    refund,
    rootsSet,
    decodedAddress,
    decodedAddress,
    u8aToHex(assetId),
    leavesMap
  );

  // now we call the vanchor transact to deposit on substrate.
  const transactCall = api.tx.vAnchorBn254.transact(
    treeId,
    data.publicInputs,
    data.extData
  );
  const txSigned = await transactCall.signAsync(account);
  await aliceNode.executeTransaction(txSigned);
  return { depositUtxos: outputUtxos };
}

async function getFixtures() {
  const gitRoot = child
    .execSync('git rev-parse --show-toplevel')
    .toString()
    .trim();

  const witnessCalculatorCjsPath_2 = path.join(
    gitRoot,
    'tests',
    'solidity-fixtures/vanchor_2/2/witness_calculator.cjs'
  );

  const witnessCalculatorCjsPath_16 = path.join(
    gitRoot,
    'tests',
    'solidity-fixtures/vanchor_16/2/witness_calculator.cjs'
  );

  const zkComponents_2 = await fetchComponentsFromFilePaths(
    path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_2/2/poseidon_vanchor_2_2.wasm'
    ),
    witnessCalculatorCjsPath_2,
    path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_2/2/circuit_final.zkey'
    )
  );

  const zkComponents_16 = await fetchComponentsFromFilePaths(
    path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_16/2/poseidon_vanchor_16_2.wasm'
    ),
    witnessCalculatorCjsPath_16,
    path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_16/2/circuit_final.zkey'
    )
  );
  return { zkComponents_2, zkComponents_16 };
}

async function setupTransaction(
  chainId: number,
  inputs: Utxo[],
  outputs: [Utxo, Utxo],
  fee: BigNumber,
  refund: BigNumber,
  roots: string[],
  recipient: string,
  relayer: string,
  wrapUnwrapToken: string,
  leavesMap: Record<string, Uint8Array[]>
): Promise<{
  extData: IVariableAnchorExtData;
  publicInputs: IVariableAnchorPublicInputs;
}> {
  // first, check if the merkle root is known on chain - if not, then update
  inputs = await padUtxos(chainId, inputs, 16);
  // outputs = await padUtxos(chainId, outputs, 2) ?? []];
  const extAmount = getExtAmount(inputs, outputs, fee);

  // calculate the sum of input notes (for calculating the public amount)
  let sumInputs: BigNumber = BigNumber.from(0);
  const leafIds: LeafIdentifier[] = [];

  for (const inputUtxo of inputs) {
    sumInputs = BigNumber.from(sumInputs).add(inputUtxo.amount);
    leafIds.push({
      index: inputUtxo.index ?? 0,
      typedChainId: Number(inputUtxo.originChainId),
    });
  }

  const encryptedCommitments: [Uint8Array, Uint8Array] = [
    hexToU8a(outputs[0].encrypt()),
    hexToU8a(outputs[1].encrypt()),
  ];

  const { zkComponents_2, zkComponents_16 } = await getFixtures();
  const proofInput: ProvingManagerSetupInput<'vanchor'> = {
    inputUtxos: inputs,
    leavesMap,
    leafIds,
    roots: roots.map((root) => hexToU8a(root)),
    chainId: chainId.toString(),
    output: [outputs[0], outputs[1]],
    encryptedCommitments,
    publicAmount: BigNumber.from(extAmount).toString(),
    provingKey: inputs.length > 2 ? zkComponents_16.zkey : zkComponents_2.zkey,
    relayer: hexToU8a(relayer),
    refund: BigNumber.from(refund).toString(),
    token: hexToU8a(wrapUnwrapToken),
    recipient: hexToU8a(recipient),
    extAmount: toFixedHex(BigNumber.from(extAmount)),
    fee: BigNumber.from(fee).toString(),
  };
  let provingManager: CircomProvingManager;
  inputs.length > 2
    ? (provingManager = new CircomProvingManager(
      zkComponents_16.wasm,
      30,
      null
    ))
    : (provingManager = new CircomProvingManager(
      zkComponents_2.wasm,
      30,
      null
    ));

  const proof = await provingManager.prove('vanchor', proofInput);
  const publicInputs: IVariableAnchorPublicInputs = generatePublicInputs(
    proof.proof,
    roots.map((root) => BigNumber.from(root)),
    inputs,
    outputs,
    BigNumber.from(proofInput.publicAmount),
    proof.extDataHash
  );

  const extData: IVariableAnchorExtData = {
    recipient: toFixedHex(proofInput.recipient, 20),
    extAmount: toFixedHex(proofInput.extAmount),
    relayer: toFixedHex(proofInput.relayer, 20),
    fee: toFixedHex(proofInput.fee),
    refund: toFixedHex(proofInput.refund),
    token: toFixedHex(proofInput.token, 20),
    encryptedOutput1: u8aToHex(proofInput.encryptedCommitments[0]),
    encryptedOutput2: u8aToHex(proofInput.encryptedCommitments[1]),
  };

  return {
    extData,
    publicInputs,
  };
}

async function padUtxos(
  chainId: number,
  utxos: Utxo[],
  maxLen: number
): Promise<Utxo[]> {
  const randomKeypair = new Keypair();

  while (utxos.length !== 2 && utxos.length < maxLen) {
    utxos.push(
      await CircomUtxo.generateUtxo({
        curve: 'Bn254',
        backend: 'Circom',
        chainId: chainId.toString(),
        originChainId: chainId.toString(),
        amount: '0',
        blinding: hexToU8a(randomBN(31).toHexString()),
        keypair: randomKeypair,
      })
    );
  }
  if (utxos.length !== 2 && utxos.length !== maxLen) {
    throw new Error('Invalid utxo length');
  }
  return utxos;
}

function getExtAmount(inputs: Utxo[], outputs: Utxo[], fee: BigNumber) {
  return BigNumber.from(fee)
    .add(outputs.reduce((sum, x) => sum.add(x.amount), BigNumber.from(0)))
    .sub(inputs.reduce((sum, x) => sum.add(x.amount), BigNumber.from(0)));
}

function generatePublicInputs(
  proof: any,
  roots: BigNumber[],
  inputs: Utxo[],
  outputs: [Utxo, Utxo],
  publicAmount: BigNumber,
  extDataHash: BigNumber
): IVariableAnchorPublicInputs {
  // public inputs to the contract
  const args: IVariableAnchorPublicInputs = {
    proof: `0x${proof}`,
    roots: `0x${roots.map((x) => toFixedHex(x).slice(2)).join('')}`,
    extensionRoots: '0x',
    inputNullifiers: inputs.map((x) =>
      BigNumber.from(toFixedHex('0x' + x.nullifier))
    ),
    outputCommitments: [
      BigNumber.from(toFixedHex(u8aToHex(outputs[0].commitment))),
      BigNumber.from(toFixedHex(u8aToHex(outputs[1].commitment))),
    ],
    publicAmount: toFixedHex(publicAmount),
    extDataHash,
  };

  return args;
}

interface IVariableAnchorExtData {
  recipient: string;
  extAmount: string;
  relayer: string;
  fee: string;
  refund: string;
  token: string;
  encryptedOutput1: string;
  encryptedOutput2: string;
}

interface IVariableAnchorPublicInputs {
  proof: string;
  roots: string;
  extensionRoots: string;
  inputNullifiers: BigNumber[];
  outputCommitments: [BigNumber, BigNumber];
  publicAmount: string;
  extDataHash: BigNumber;
}
