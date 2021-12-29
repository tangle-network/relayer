import circomlib from 'circomlib';
import crypto from 'crypto';
import { ethers } from 'ethers';
import fs from 'fs';
import snarkjs from 'snarkjs';
import websnarkUtils from 'websnark/src/utils';
import buildGroth16 from 'websnark/src/groth16';
import tornadoContract from './build/contracts/Anchor.json';
import nativeTornadoContract from './build/contracts/NativeAnchor.json';
import MerkleTree from './lib/MerkleTree';
import fetch from 'node-fetch';
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Note, NoteGenInput } from '@webb-tools/sdk-mixer';
// import { ProvingManger, ProvingManagerWrapper } from '@webb-tools/sdk-core';
// import Worker from "./mixer.worker.ts";
import { Keyring } from "@polkadot/keyring";
import { options } from '@webb-tools/api';

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

export async function getTornadoDenomination(
  contractAddress: string,
  provider: ethers.providers.Provider
): Promise<string> {
  const nativeTornadoInstance = new ethers.Contract(
    contractAddress,
    nativeTornadoContract.abi,
    provider
  );

  const denomination = await nativeTornadoInstance.functions.denomination!();
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

function createTornadoDeposit() {
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

export async function depositTornado(contractAddress: string, wallet: ethers.Signer) {
  const deposit = createTornadoDeposit();
  console.log('deposit created');
  const nativeTornadoInstance = new ethers.Contract(
    contractAddress,
    nativeTornadoContract.abi,
    wallet
  );
  const toFixedHex = (number: number | Buffer, length = 32) =>
    '0x' +
    (number instanceof Buffer
      ? number.toString('hex')
      : snarkjs.bigInt(number).toString(16)
    ).padStart(length * 2, '0');

  const denomination = await nativeTornadoInstance.functions.denomination!();

  console.log(denomination.toString());
  // Gas limit values required for beresheet
  const depositTx = await nativeTornadoInstance.deposit(
    toFixedHex(deposit.commitment),
    {
      value: denomination.toString(),
      gasLimit: 6000000,
    }
  );
  await depositTx.wait();
  return deposit;
}

export async function withdrawTornado(contractAddress: string, proof: any, args: any, wallet: ethers.Signer) {
  const nativeTornadoInstance = new ethers.Contract(
    contractAddress,
    nativeTornadoContract.abi,
    wallet
  );

  const withdrawTx = await nativeTornadoInstance.withdraw(
    proof, 
    ...args,
    {
      gasLimit: 6000000
    }
  );
  const receipt = await withdrawTx.wait();
  return receipt;
}

export async function getDepositLeavesFromChain(
  contractAddress: string,
  provider: ethers.providers.Provider
) {
  // Query the blockchain for all deposits that have happened
  const TornadoInterface = new ethers.utils.Interface(tornadoContract.abi);
  const TornadoInstance = new ethers.Contract(
    contractAddress,
    tornadoContract.abi,
    provider
  );
  const depositFilterResult = TornadoInstance.filters.Deposit!();
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
    return TornadoInterface.parseLog(log);
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

  const jsonResponse: any = await relayerResponse.json();
  let leaves = jsonResponse.leaves;

  return leaves;
}

export async function generateTornadoMerkleProof(leaves: any, deposit: any) {
  const tree = new MerkleTree(20, leaves);

  let leafIndex = leaves.findIndex((e) => e === toHex(deposit.commitment));

  const retVals = await tree.path(leafIndex);

  return retVals;
}

export async function generateTornadoSnarkProof(
  leaves: any,
  deposit: any,
  recipient: string,
  relayer: string,
  fee: string
) {
  // find the inputs that correspond to the path for the deposit
  const { root, path_elements, path_index } = await generateTornadoMerkleProof(
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

export async function generateSubstrateMixerSnarkProof(
  leaves: any,
  deposit: any,
  recipient: string,
  relayer: string,
  fee: string
) {
  return;
}

export async function createSubstrateDeposit() {
  const chainId = 1080;
  const noteInput: NoteGenInput = {
    prefix: 'webb.mix',
    version: 'v1',
    exponentiation: '5',
    width: '3',
    backend: 'Arkworks',
    hashFunction: 'Poseidon',
    curve: 'Bn254',
    denomination: `18`,

    amount: String(10),
    chain: String(chainId),
    sourceChain: String(chainId),
    tokenSymbol: 'WEBB',
  };
  const depositNote = await Note.generateNote(noteInput);
  return depositNote;
}

export const provider = new WsProvider('ws://127.0.0.1:9944');
export const ALICE = '5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY';

export async function getDepositLeavesFromSubstrateChain(treeId: string) {
  const api = new ApiPromise(options({ provider }));
  await api.isReady;
}

export async function depositSubstrateMixer(_treeId: string) {
  const deposit = await createSubstrateDeposit();
  const keyring = new Keyring({ type: "sr25519" });
	const alice = keyring.addFromUri("//Alice");
  const api = new ApiPromise(options({ provider }));
  await api.isReady;
  console.log(api);
  const toFixedHex = (number: number | Buffer, length = 32) =>
    '0x' +
    (number instanceof Buffer
      ? number.toString('hex')
      : snarkjs.bigInt(number).toString(16)
    ).padStart(length * 2, '0');

  // Gas limit values required for beresheet
  // @ts-ignore
  const unsub = await api.tx.mixerBn254.deposit(
    deposit.getLeaf()
  ).signAndSend(alice, ({ events = [], status }) => {
		console.log(`Current status is: ${status.type}`);

		if (status.isFinalized) {
			console.log(`Transaction included at blockHash ${status.asFinalized}`);

			events.forEach(({ phase, event: { data, method, section } }) => {
				console.log(`\t' ${phase}: ${section}.${method}:: ${data}`);
			});

			unsub();
		}
  });
  return deposit;
}

export async function withdrawSubstrateMixer(treeId: string, proof: any, args: any) {
  const keyring = new Keyring({ type: "sr25519" });
	const alice = keyring.addFromUri("//Alice");
  const api = new ApiPromise(options({ provider }));
  await api.isReady;

  // @ts-ignore
	const unsub = await api.tx.mixerBn254.withdraw(
    treeId,
    proof, 
    ...args
  ).signAndSend(alice, ({ events = [], status }) => {
		console.log(`Current status is: ${status.type}`);

		if (status.isFinalized) {
			console.log(`Transaction included at blockHash ${status.asFinalized}`);

			events.forEach(({ phase, event: { data, method, section } }) => {
				console.log(`\t' ${phase}: ${section}.${method}:: ${data}`);
			});

			unsub();
		}
  });
}