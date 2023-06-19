// Helper methods, we can move them somewhere if we end up using them again.

import assert from 'node:assert';
import path from 'node:path';
import child from 'node:child_process';

import { hexToU8a, u8aToHex } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import {
  Keypair,
  MerkleTree,
  Utxo,
  CircomUtxo,
  randomBN,
  toFixedHex,
  buildVariableWitnessCalculator,
  generateVariableWitnessInput,
  type LeafIdentifier,
  type MerkleProof,
} from '@webb-tools/sdk-core';
import { currencyToUnitI128 } from '@webb-tools/test-utils';
import * as snarkjs from 'snarkjs';
import { BigNumber, ethers } from 'ethers';
import { LocalTangle } from './localTangle.js';
import { createAccount } from './utils.js';
import { fetchComponentsFromFilePaths, FIELD_SIZE } from '@webb-tools/utils';
import { type KeyringPair } from '@polkadot/keyring/types';
import { type IVariableAnchorExtData } from '@webb-tools/interfaces';
import { HexString } from '@polkadot/util/types';

export async function vanchorDeposit(
  account: KeyringPair,
  treeId: number,
  typedTargetChainId: string,
  typedSourceChainId: string,
  publicAmountUint: number,
  aliceNode: LocalTangle
): Promise<{ depositUtxos: [Utxo, Utxo] }> {
  const api = await aliceNode.api();
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
  const fee = BigNumber.from(0);
  const refund = BigNumber.from(0);
  // Initially leaves will be empty
  leavesMap[typedTargetChainId.toString()] = [];
  const tree = await api.query.merkleTreeBn254.trees(treeId);
  const vanchor = await api.query.vAnchorBn254.vAnchors(treeId);
  const asset = vanchor.unwrap().asset;
  const assetId = asset.toU8a();
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

export async function vanchorWithdraw(
  treeId: number,
  typedTargetChainId: string,
  typedSourceChainId: string,
  depositUtxos: [Utxo, Utxo],
  aliceNode: LocalTangle
): Promise<{ outputUtxos: [Utxo, Utxo] }> {
  const api = await aliceNode.api();
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

export async function setupTransaction(
  typedChainId: number,
  inputs: Utxo[],
  outputs: [Utxo, Utxo],
  fee: BigNumber,
  refund: BigNumber,
  roots: Uint8Array[],
  recipient: HexString,
  relayer: HexString,
  wrapUnwrapToken: HexString,
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
  return {
    extData,
    publicInputs,
  };
}

export function generateExtData(
  recipient: HexString,
  relayer: HexString,
  extAmount: bigint,
  fee: bigint,
  refund: bigint,
  wrapUnwrapToken: HexString,
  encryptedOutput1: HexString,
  encryptedOutput2: HexString
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

export function getVAnchorExtDataHash(extData: IVariableAnchorExtData): bigint {
  const abi = new ethers.utils.AbiCoder();
  const encodedData = abi.encode(
    [
      '(bytes recipient,int256 extAmount,bytes relayer,uint256 fee,uint256 refund,bytes token,bytes encryptedOutput1,bytes encryptedOutput2)',
    ],
    [extData]
  );
  let hash = ethers.utils.keccak256(encodedData);
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

export async function generatePublicInputs(
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

export function getExtAmount(inputs: Utxo[], outputs: Utxo[], fee: BigNumber) {
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

export async function groth16ProofToBytes(
  proof: Groth16Proof
): Promise<Uint8Array> {
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
  return hexToU8a(encodedData);
}
