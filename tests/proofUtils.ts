import circomlib from 'circomlib';
import crypto from 'crypto';
import { ethers } from 'ethers';
import fs from 'fs';
import snarkjs from 'snarkjs';
import websnarkUtils from 'websnark/src/utils';
import buildGroth16 from 'websnark/src/groth16';
import anchorContract from './build/contracts/Anchor.json';
import nativeAnchorContract from './build/contracts/NativeAnchor.json';
import MerkleTree from './lib/MerkleTree';
import fetch from 'node-fetch';

// variable to hold groth16 and not reinitialize
let groth16;

export type Deposit = {
  nullifier: snarkjs.bigInt;
  secret: snarkjs.bigInt;
  preimage: Buffer;
  commitment: snarkjs.bigInt;
  nullifierHash: snarkjs.bigInt;
}

export const toHex = (number: number | Buffer, length = 32) =>
  '0x' +
  (number instanceof Buffer
    ? number.toString('hex')
    : snarkjs.bigInt(number).toString(16)
  ).padStart(length * 2, '0');

export async function getAnchorDenomination(
  contractAddress: string,
  provider: ethers.providers.Provider
): Promise<string> {
  const nativeAnchorInstance = new ethers.Contract(
    contractAddress,
    nativeAnchorContract.abi,
    provider
  );

  const denomination = await nativeAnchorInstance.functions.denomination!();
  return denomination;
}

// BigNumber for principle
export const calculateFee = (
  withdrawFeePercentage: number,
  principle: string
): string => {
  const principleBig = snarkjs.bigInt(principle);
  const withdrawFeeMill = withdrawFeePercentage * 1000000;
  const withdrawFeeMillBig = snarkjs.bigInt(withdrawFeeMill);
  const feeBigMill = principleBig * withdrawFeeMillBig;
  const feeBig = feeBigMill / snarkjs.bigInt(1000000);
  const fee = feeBig.toString();

  return fee;
};

function createDeposit() {
  const rbigint = (nbytes: number) =>
    snarkjs.bigInt.leBuff2int(crypto.randomBytes(nbytes));
  const pedersenHash = (data: any) =>
    circomlib.babyJub.unpackPoint(circomlib.pedersenHash.hash(data))[0];
  let deposit: any = { nullifier: rbigint(31), secret: rbigint(31) };
  deposit.preimage = Buffer.concat([
    deposit.nullifier.leInt2Buff(31),
    deposit.secret.leInt2Buff(31),
  ]);
  deposit.commitment = pedersenHash(deposit.preimage);
  deposit.nullifierHash = pedersenHash(deposit.nullifier.leInt2Buff(31));
  return deposit;
}

export async function deposit(contractAddress: string, wallet: ethers.Signer) {
  const deposit = createDeposit();
  console.log('deposit created');
  const nativeAnchorInstance = new ethers.Contract(
    contractAddress,
    nativeAnchorContract.abi,
    wallet
  );
  const toFixedHex = (number: number | Buffer, length = 32) =>
    '0x' +
    (number instanceof Buffer
      ? number.toString('hex')
      : snarkjs.bigInt(number).toString(16)
    ).padStart(length * 2, '0');

  const denomination = await nativeAnchorInstance.functions.denomination!();

  console.log(denomination.toString());
  // Gas limit values required for beresheet
  const depositTx = await nativeAnchorInstance.deposit(
    toFixedHex(deposit.commitment),
    {
      value: denomination.toString(),
      gasLimit: 6000000,
    }
  );
  await depositTx.wait();
  return deposit;
}

export async function getDepositLeavesFromChain(
  contractAddress: string,
  provider: ethers.providers.Provider
) {
  // Query the blockchain for all deposits that have happened
  const anchorInterface = new ethers.utils.Interface(anchorContract.abi);
  const anchorInstance = new ethers.Contract(
    contractAddress,
    anchorContract.abi,
    provider
  );
  const depositFilterResult = anchorInstance.filters.Deposit!();
  const currentBlock = await provider.getBlockNumber();

  const logs = await provider.getLogs({
    fromBlock: currentBlock - 1000,
    toBlock: currentBlock,
    address: contractAddress,
    //@ts-ignore
    topics: [depositFilterResult.topics],
  });

  // Decode the logs for deposit events
  const decodedEvents = logs.map((log) => {
    return anchorInterface.parseLog(log);
  });

  // format the decoded events into a sorted array of leaves.
  const leaves = decodedEvents
    .sort((a, b) => a.args.leafIndex - b.args.leafIndex) // Sort events in chronological order
    .map((e) => e.args.commitment);

  return leaves;
}

export async function getDepositLeavesFromRelayer(
  contractAddress: string,
  endpoint: string
): Promise<string[]> {
  const relayerResponse = await fetch(`${endpoint}/api/v1/leaves/${contractAddress}`);

  const jsonResponse = await relayerResponse.json();
  let leaves = jsonResponse.leaves;

  return leaves;
}

export async function generateMerkleProof(leaves: any, deposit: any) {
  const tree = new MerkleTree(20, leaves);

  let leafIndex = leaves.findIndex((e) => e === toHex(deposit.commitment));

  const retVals = await tree.path(leafIndex);

  return retVals;
}

export async function generateSnarkProof(
  leaves: any,
  deposit: any,
  recipient: string,
  relayer: string,
  fee: string
) {
  // find the inputs that correspond to the path for the deposit
  const { root, path_elements, path_index } = await generateMerkleProof(
    leaves,
    deposit
  );

  // Only build groth16 once
  if (!groth16) {
    groth16 = await buildGroth16();
  }
  let circuit = require('./build/circuits/withdraw.json');
  let proving_key = fs.readFileSync(
    './build/circuits/withdraw_proving_key.bin'
  ).buffer;

  // Circuit input
  const input = {
    // public
    root: root,
    nullifierHash: deposit.nullifierHash,
    relayer: snarkjs.bigInt(relayer),
    recipient: snarkjs.bigInt(recipient),
    fee: snarkjs.bigInt(fee),
    refund: 0,

    // private
    nullifier: deposit.nullifier,
    secret: deposit.secret,
    pathElements: path_elements,
    pathIndices: path_index,
  };

  const proofData = await websnarkUtils.genWitnessAndProve(
    groth16,
    input,
    circuit,
    proving_key
  );
  const { proof } = websnarkUtils.toSolidityInput(proofData);

  const args = [
    toHex(input.root),
    toHex(input.nullifierHash),
    toHex(input.recipient, 20),
    toHex(input.relayer, 20),
    toHex(input.fee),
    toHex(input.refund),
  ];

  return { proof, args };
}
