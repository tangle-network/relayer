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

import { assert } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import isCi from 'is-ci';
import child from 'child_process';
import * as snarkjs from 'snarkjs';
import { WebbRelayer, Pallet } from '../../lib/webbRelayer.js';
import { BigNumber, ethers } from 'ethers';
import { ApiPromise } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import { IVariableAnchorExtData } from '@webb-tools/interfaces';
import {
  Utxo,
  calculateTypedChainId,
  ChainType,
  Keypair,
  CircomUtxo,
  randomBN,
  toFixedHex,
  LeafIdentifier,
  FIELD_SIZE,
  buildVariableWitnessCalculator,
  generateVariableWitnessInput,
  MerkleProof,
  MerkleTree,
} from '@webb-tools/sdk-core';
import { currencyToUnitI128, UsageMode } from '@webb-tools/test-utils';
import { createAccount, defaultEventsWatcherValue } from '../../lib/utils.js';
import { LocalTangle } from '../../lib/localTangle.js';
import { fetchComponentsFromFilePaths } from '@webb-tools/utils';
import { HexString } from '@polkadot/util/types';

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
      enableLogging: true,
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
      showLogs: false,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should withdraw using private transaction ', async () => {
    const api = await aliceNode.api();
    // 1. Create vanchor on Substrate chain with height 30 and maxEdges = 1
    const createVAnchorCall = api.tx.vAnchorBn254.create!(1, 30, 0);
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
    const data = await vanchorDeposit(
      typedSourceChainId.toString(), // source chain Id
      typedSourceChainId.toString(), // target chain Id
      1000000000, // public amount
      treeId,
      api,
      aliceNode
    );
    console.log('data', data);

    // 3. Now we withdraw it.
    const data2 = await vanchorWithdraw(
      typedSourceChainId.toString(),
      typedSourceChainId.toString(),
      data.depositUtxos,
      treeId,
      api,
      aliceNode
    );

    console.log('data2', data2);

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

async function vanchorDeposit(
  typedTargetChainId: string,
  typedSourceChainId: string,
  publicAmountUint: number,
  treeId: number,
  api: ApiPromise,
  aliceNode: LocalTangle
): Promise<{ depositUtxos: [Utxo, Utxo] }> {
  const account = createAccount('//Dave');

  const publicAmount = currencyToUnitI128(publicAmountUint);
  // Output UTXOs configs
  const output1 = await CircomUtxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Circom',
    amount: publicAmount.toString(),
    chainId: typedTargetChainId,
  });
  const output2 = await CircomUtxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Circom',
    amount: '0',
    chainId: typedTargetChainId,
  });

  const inputUtxos = [];
  const outputUtxos: [Utxo, Utxo] = [output1, output2];

  const leavesMap = {};

  const address = account.address;
  const assetId = new Uint8Array([0, 0, 0, 0]); // WEBB native token asset Id.
  const fee = BigNumber.from(0);
  const refund = BigNumber.from(0);
  // Initially leaves will be empty
  leavesMap[typedTargetChainId.toString()] = [];
  const tree = await api.query.merkleTreeBn254.trees(treeId);
  //@ts-ignore
  const root = tree.unwrap().root.toHex();
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  const neighborRoots: string[] = await api.rpc.lt
    .getNeighborRoots(treeId)
    .then((roots: any) => roots.toHuman());

  const rootsSet = [hexToU8a(root), hexToU8a(neighborRoots[0])];
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

async function vanchorWithdraw(
  typedTargetChainId: string,
  typedSourceChainId: string,
  depositUtxos: [Utxo, Utxo],
  treeId: number,
  api: ApiPromise,
  aliceNode: LocalTangle
): Promise<{ outputUtxos: [Utxo, Utxo] }> {
  const account = createAccount('//Dave');

  const inputUtxos = depositUtxos;
  const outputUtxos = await padUtxos(Number(typedSourceChainId), [], 2);

  const leavesMap = {};

  const address = account.address;
  const assetId = new Uint8Array([0, 0, 0, 0]); // WEBB native token asset Id.
  const fee = BigNumber.from(0);
  const refund = BigNumber.from(0);
  // Initially leaves will be empty
  leavesMap[typedSourceChainId.toString()] = [];
  const tree = await api.query.merkleTreeBn254.trees(treeId);
  //@ts-ignore
  const root = tree.unwrap().root.toHex();
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  const neighborRoots: string[] = await api.rpc.lt
    .getNeighborRoots(treeId)
    .then((roots: any) => roots.toHuman());

  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  const leaves: string[] = await api.rpc.mt
    .getLeaves(treeId, 0, 256)
    .then((leaves: any) => leaves.toHuman());
  leavesMap[typedTargetChainId.toString()] = leaves;

  const rootsSet = [hexToU8a(root), hexToU8a(neighborRoots[0])];
  const decodedAddress = u8aToHex(decodeAddress(address));

  const data = await setupTransaction(
    Number(typedTargetChainId),
    inputUtxos,
    outputUtxos as [Utxo, Utxo],
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
  return { outputUtxos: outputUtxos as [Utxo, Utxo] };
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
  typedChainId: number,
  inputs: Utxo[],
  outputs: [Utxo, Utxo],
  fee: BigNumber,
  refund: BigNumber,
  roots: Uint8Array[],
  recipient: string,
  relayer: string,
  wrapUnwrapToken: string,
  leavesMap: Record<string, Uint8Array[]>
): Promise<{
  extData: IVariableAnchorExtData;
  publicInputs: IVAnchorPublicInputs;
}> {
  // first, check if the merkle root is known on chain - if not, then update
  inputs = await padUtxos(typedChainId, inputs, 16);

  if (wrapUnwrapToken.length === 0) {
    throw new Error('No asset id provided');
  }

  if (outputs.length !== 2) {
    throw new Error('Only two outputs are supported');
  }
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

  const { zkComponents_2 } = await getFixtures();

  const fieldSize = FIELD_SIZE.toBigInt();
  const publicAmount =
    (extAmount.toBigInt() - fee.toBigInt() + fieldSize) % fieldSize;

  const { extData, extDataHash } = generateExtData(
    recipient,
    relayer,
    extAmount.toBigInt(),
    fee.toBigInt(),
    refund.toBigInt(),
    wrapUnwrapToken,
    u8aToHex(encryptedCommitments[0]),
    u8aToHex(encryptedCommitments[1])
  );

  const proofInputs = await generateProofInputs(
    roots.map((r) => BigInt(u8aToHex(r))),
    BigInt(typedChainId),
    inputs,
    outputs,
    publicAmount,
    fee.toBigInt(),
    extDataHash,
    leavesMap
  );

  const witness = await getSnarkJsWitness(proofInputs, zkComponents_2.wasm);

  const proof = await getSnarkJsProof(zkComponents_2.zkey, witness);

  const publicInputs = await generatePublicInputs(
    proof,
    roots,
    inputs,
    outputs,
    publicAmount,
    extDataHash
  );
  // For Substrate, specifically, we need the extAmount to not be in hex value,
  // since substrate does not understand -ve hex values.
  // Hence, we convert it to a string.
  extData.extAmount = extAmount.toString();
  console.log('extData', extData);
  console.log('extDataHashBigInt', extDataHash);
  console.log('proofInputs', proofInputs);
  console.log('publicInputs', publicInputs);
  return {
    extData,
    publicInputs,
  };
}

function generateExtData(
  recipient: string,
  relayer: string,
  extAmount: bigint,
  fee: bigint,
  refund: bigint,
  wrapUnwrapToken: string,
  encryptedOutput1: string,
  encryptedOutput2: string
): {
  extData: IVariableAnchorExtData;
  extDataHash: bigint;
} {
  const extData: IVariableAnchorExtData = {
    // For recipient, since it is an AccountId (32 bytes) we use toFixedHex to pad it to 32 bytes.
    recipient: toFixedHex(recipient, 32),
    // For relayer, since it is an AccountId (32 bytes) we use toFixedHex to pad it to 32 bytes.
    relayer: toFixedHex(relayer, 32),
    // For extAmount, since it is an Amount (i128) it should be 16 bytes
    extAmount: toFixedHex(BigNumber.from(extAmount).toTwos(128), 16),
    // For fee, since it is a Balance (u128) it should be 16 bytes
    fee: toFixedHex(BigNumber.from(fee).toTwos(128), 16),
    // For refund, since it is a Balance (u128) it should be 16 bytes
    refund: toFixedHex(BigNumber.from(refund).toTwos(128), 16),
    // For token, since it is an AssetId (u32) it should be 4 bytes
    token: toFixedHex(wrapUnwrapToken, 4),
    encryptedOutput1,
    encryptedOutput2,
  };

  const extDataHash = getVAnchorExtDataHash(extData);

  return { extData, extDataHash };
}

function getVAnchorExtDataHash(extData: IVariableAnchorExtData): bigint {
  const abi = new ethers.utils.AbiCoder();
  const encodedData = abi.encode(
    [
      '(bytes recipient,int256 extAmount,bytes relayer,uint256 fee,uint256 refund,bytes token,bytes encryptedOutput1,bytes encryptedOutput2)',
    ],
    [extData]
  );
  console.log('encodedData', encodedData);
  console.log('extAmount', BigNumber.from(extData.extAmount).toHexString());
  let hash = ethers.utils.keccak256(encodedData);
  console.log('hash1', hash);
  let hashBigInt = BigNumber.from(hash);
  return hashBigInt.mod(FIELD_SIZE).toBigInt();
}

function ensureEvenLength(maybeHex: string): string {
  // strip 0x prefix if present
  const input = maybeHex.startsWith('0x') ? maybeHex.slice(2) : maybeHex;
  return input.length % 2 === 0 ? input : `0${input}`;
}

function ensureHex(maybeHex: string): `0x${string}` {
  const input = ensureEvenLength(maybeHex);
  return `0x${input}`;
}

async function generateProofInputs(
  roots: bigint[],
  typedChainId: bigint,
  inputUtxos: Utxo[],
  outputUtxos: Utxo[],
  publicAmount: bigint,
  fee: bigint,
  extDataHash: bigint,
  leavesMap: Record<string, Uint8Array[]>
) {
  const levels = 30;

  let vanchorMerkleProof: MerkleProof[];
  if (Object.keys(leavesMap).length === 0) {
    vanchorMerkleProof = inputUtxos.map((u) => getMerkleProof(levels, u));
  } else {
    const treeElements = leavesMap[typedChainId.toString()];
    vanchorMerkleProof = inputUtxos.map((u) =>
      getMerkleProof(levels, u, treeElements)
    );
  }

  const witnessInput = generateVariableWitnessInput(
    roots.map((r) => BigNumber.from(r)), // Temporary use of `BigNumber`, need to change to `BigInt`
    typedChainId,
    inputUtxos,
    outputUtxos,
    publicAmount,
    fee,
    BigNumber.from(extDataHash), // Temporary use of `BigNumber`, need to change to `BigInt`
    vanchorMerkleProof
  );

  return witnessInput;
}

async function generatePublicInputs(
  proof: Groth16Proof,
  roots: Uint8Array[],
  inputs: Utxo[],
  outputs: Utxo[],
  publicAmount: bigint,
  extDataHash: bigint
): Promise<IVAnchorPublicInputs> {
  // convert the proof object into arkworks serialization format.
  const proofBytes = await groth16ProofToBytes(proof);
  const publicAmountHex = ensureHex(toFixedHex(publicAmount));
  const extDataHashHex = ensureHex(toFixedHex(extDataHash));
  const inputNullifiers = inputs.map((x) =>
    ensureHex(toFixedHex('0x' + x.nullifier))
  );
  const outputCommitments = [
    ensureHex(toFixedHex(u8aToHex(outputs[0]!.commitment))),
    ensureHex(toFixedHex(u8aToHex(outputs[1]!.commitment))),
  ];
  const rootsHex = roots.map((r) => ensureHex(toFixedHex(u8aToHex(r))));
  console.log('Grot16 proof: ', proof.proof);
  console.log('Groth16 proof bytes:', u8aToHex(proofBytes));
  const publicInputsList = [
    publicAmountHex,
    extDataHashHex,
    ...inputNullifiers,
    ...outputCommitments,
    ...rootsHex,
  ];
  console.log('Public inputs: ', publicInputsList);
  return {
    proof: ensureHex(u8aToHex(proofBytes)),
    roots: rootsHex,
    inputNullifiers,
    outputCommitments: [outputCommitments[0]!, outputCommitments[1]!],
    publicAmount: publicAmountHex,
    extDataHash: extDataHashHex,
  };
}

async function getSnarkJsWitness(
  witnessInput: any,
  circuitWasm: Buffer
): Promise<Uint8Array> {
  const witnessCalculator = await buildVariableWitnessCalculator(
    circuitWasm,
    0
  );
  return witnessCalculator.calculateWTNSBin(witnessInput, 0);
}

async function getSnarkJsProof(
  zkey: Uint8Array,
  witness: Uint8Array
): Promise<Groth16Proof> {
  const proofOutput: Groth16Proof = await snarkjs.groth16.prove(zkey, witness);

  const proof = proofOutput.proof;
  const publicSignals = proofOutput.publicSignals;

  const vKey = await snarkjs.zKey.exportVerificationKey(zkey);

  const isValid = await snarkjs.groth16.verify(vKey, publicSignals, proof);
  console.log('Proof is valid: ', isValid);
  assert.strictEqual(isValid, true, 'Invalid proof');

  return proofOutput;
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

function getMerkleProof(
  levels: number,
  input: Utxo,
  leavesMap?: Uint8Array[]
): MerkleProof {
  const tree = new MerkleTree(levels, leavesMap);

  let inputMerklePathIndices: number[];
  let inputMerklePathElements: BigNumber[];

  if (Number(input.amount) > 0) {
    if (input.index === undefined) {
      throw new Error(
        `Input commitment ${u8aToHex(input.commitment)} index was not set`
      );
    }
    if (input.index < 0) {
      throw new Error(
        `Input commitment ${u8aToHex(input.commitment)} index should be >= 0`
      );
    }
    if (leavesMap === undefined) {
      const path = tree.path(input.index);
      inputMerklePathIndices = path?.pathIndices;
      inputMerklePathElements = path?.pathElements;
    } else {
      const mt = new MerkleTree(levels, leavesMap);
      const path = mt.path(input.index);
      inputMerklePathIndices = path?.pathIndices;
      inputMerklePathElements = path?.pathElements;
    }
  } else {
    inputMerklePathIndices = new Array(tree.levels).fill(0);
    inputMerklePathElements = new Array(tree.levels).fill(0);
  }

  return {
    element: BigNumber.from(u8aToHex(input.commitment)),
    pathElements: inputMerklePathElements,
    pathIndices: inputMerklePathIndices,
    merkleRoot: tree.root(),
  };
}

interface Groth16Proof {
  proof: {
    pi_a: [string, string];
    pi_b: [[string, string], [string, string]];
    pi_c: [string, string];
    curve: string;
    prococol: 'groth16';
  };
  publicSignals: string[];
}

interface IVAnchorPublicInputs {
  proof: HexString;
  roots: HexString[];
  inputNullifiers: HexString[];
  outputCommitments: [HexString, HexString];
  publicAmount: HexString;
  extDataHash: HexString;
}

async function groth16ProofToBytes(proof: Groth16Proof): Promise<Uint8Array> {
  const callData = await snarkjs.groth16.exportSolidityCallData(
    proof.proof,
    proof.publicSignals
  );

  const parsedCalldata = JSON.parse('[' + callData + ']');
  const pi_a = parsedCalldata[0] as Groth16Proof['proof']['pi_a'];
  const pi_b = parsedCalldata[1] as Groth16Proof['proof']['pi_b'];
  const pi_c = parsedCalldata[2] as Groth16Proof['proof']['pi_c'];

  const abi = new ethers.utils.AbiCoder();
  const fnAbi = '(uint[2] a,uint[2][2] b,uint[2] c)';
  const encodedData = abi.encode(
    [fnAbi],
    [
      {
        a: pi_a,
        b: pi_b,
        c: pi_c,
      },
    ]
  );
  // for debugging
  const decodedData = abi.decode([fnAbi], encodedData);
  const data = decodedData[0];
  const a = data.a.map((x: BigNumber) => x.toString());
  const b = data.b.map((x: BigNumber[]) => x.map((y) => y.toString()));
  const c = data.c.map((x: BigNumber) => x.toString());
  console.log({ a, b, c });
  return hexToU8a(encodedData);
}
