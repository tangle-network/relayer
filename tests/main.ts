import circomlib from 'circomlib';
import crypto from 'crypto';
import { ethers } from 'ethers';
import fs from 'fs';
import chai from 'chai';
import WebSocket from 'ws';
import ganache from 'ganache-cli';
import snarkjs from 'snarkjs';
import websnarkUtils from 'websnark/src/utils';
import buildGroth16 from 'websnark/src/groth16';
import anchorContract from './build/contracts/Anchor.json';
import nativeAnchorContract from './build/contracts/NativeAnchor.json';
import verifierContract from './build/contracts/Verifier.json';
import MerkleTree from './lib/MerkleTree';
import { spawn } from 'child_process';

const PRIVATE_KEY =
  '0x000000000000000000000000000000000000000000000000000000000000dead';
const PORT = 1998;
const ganacheServer = ganache.server({
  accounts: [
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: PRIVATE_KEY,
    },
  ],
  port: PORT,
});

ganacheServer.listen(PORT);
const provider = new ethers.providers.JsonRpcProvider(
  `http://127.0.0.1:${PORT}`
);
console.log(`Ganache Started on http://127.0.0.1:${PORT} ..`);
const wallet = new ethers.Wallet(PRIVATE_KEY, provider);
const toHex = (number: number | Buffer, length = 32) =>
  '0x' +
  (number instanceof Buffer
    ? number.toString('hex')
    : snarkjs.bigInt(number).toString(16)
  ).padStart(length * 2, '0');

async function deployNativeAnchor() {
  const genContract = require('circomlib/src/mimcsponge_gencontract.js');

  const hasherContractRaw = {
    contractName: 'Hasher',
    abi: genContract.abi,
    bytecode: genContract.createCode('mimcsponge', 220),
  };

  const verifierContractRaw = {
    contractName: 'Verifier',
    abi: verifierContract.abi,
    bytecode: verifierContract.bytecode,
  };

  const nativeAnchorContractRaw = {
    contractName: 'NativeAnchor',
    abi: nativeAnchorContract.abi,
    bytecode: nativeAnchorContract.bytecode,
  };

  const hasherFactory = new ethers.ContractFactory(
    hasherContractRaw.abi,
    hasherContractRaw.bytecode,
    wallet
  );
  let hasherInstance = await hasherFactory.deploy({ gasLimit: '0x5B8D80' });
  await hasherInstance.deployed();

  const verifierFactory = new ethers.ContractFactory(
    verifierContractRaw.abi,
    verifierContractRaw.bytecode,
    wallet
  );
  let verifierInstance = await verifierFactory.deploy({ gasLimit: '0x5B8D80' });
  await verifierInstance.deployed();

  const denomination = ethers.utils.parseEther('1');
  const merkleTreeHeight = 20;
  const nativeAnchorFactory = new ethers.ContractFactory(
    nativeAnchorContractRaw.abi,
    nativeAnchorContractRaw.bytecode,
    wallet
  );
  let nativeAnchorInstance = await nativeAnchorFactory.deploy(
    verifierInstance.address,
    hasherInstance.address,
    denomination,
    merkleTreeHeight,
    { gasLimit: '0x5B8D80' }
  );
  const nativeAnchorAddress = await nativeAnchorInstance.deployed();

  return nativeAnchorAddress.address;
}

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

async function deposit(contractAddress: string) {
  const deposit = createDeposit();
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
  await nativeAnchorInstance.deposit(toFixedHex(deposit.commitment), {
    value: denomination.toString(),
  });
  return deposit;
}

async function getDepositEvents(contractAddress: string) {
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

  return decodedEvents;
}

async function generateMerkleProof(deposit: any, contractAddress: string) {
  const events = await getDepositEvents(contractAddress);
  const leaves = events
    .sort((a, b) => a.args.leafIndex - b.args.leafIndex) // Sort events in chronological order
    .map((e) => e.args.commitment);
  const tree = new MerkleTree(20, leaves);

  let depositEvent = events.find(
    (e) => e.args.commitment === toHex(deposit.commitment)
  );
  let leafIndex = depositEvent ? depositEvent.args.leafIndex : -1;

  const retVals = await tree.path(leafIndex);

  return retVals;
}

async function generateSnarkProof(
  deposit: any,
  recipient: string,
  contractAddress: string
) {
  // find the inputs that correspond to the path for the deposit
  const { root, path_elements, path_index } = await generateMerkleProof(
    deposit,
    contractAddress
  );

  let groth16 = await buildGroth16();
  let circuit = require('./build/circuits/withdraw.json');
  let proving_key = fs.readFileSync(
    './build/circuits/withdraw_proving_key.bin'
  ).buffer;

  // Circuit input
  const input = {
    // public
    root: root,
    nullifierHash: deposit.nullifierHash,
    relayer: 0,
    recipient: snarkjs.bigInt(recipient),
    fee: 0,
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

async function startWebbRelayer() {
  const proc = spawn('../target/debug/webb-relayer', [
    '-vvvv',
    '-c',
    './config.toml',
  ]);
  proc.stdout.on('data', (data) => {
    console.log(data.toString());
  });

  proc.stderr.on('data', (data) => {
    console.error(data.toString());
  });

  proc.on('close', (code) => {
    console.log(`relayer process exited with code ${code}`);
  });

  return proc;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

enum Result {
  Continue,
  CleanExit,
  Errored,
}

async function handleMessage(data: any): Promise<Result> {
  if (data.error) {
    return Result.Errored;
  } else if (data.withdraw?.finlized) {
    return Result.CleanExit;
  } else {
    return Result.Continue;
  }
}

async function main() {
  console.log('Deploying the contract with 1 ETH');
  const contractAddress = await deployNativeAnchor();
  console.log('Contract Deployed at ', contractAddress);
  console.log('Sending Deposit Tx to the contract ..');
  const depositArgs = await deposit(contractAddress);
  const recipient = ethers.utils.getAddress(
    '0x6e401d8f8058707b99ca54b8295a16f525070df9'
  );
  console.log('Deposit Done ..');
  console.log('Starting the Relayer ..');
  const relayer = await startWebbRelayer();
  await sleep(500); // just to wait for the relayer start-up
  const client = new WebSocket('ws://localhost:9955');
  await new Promise((resolve) => client.on('open', resolve));
  console.log('Connected to Relayer!');
  client.on('message', async (data) => {
    console.log('<==', data);
    const msg = JSON.parse(data as string);
    const result = await handleMessage(msg);
    if (result === Result.Errored) {
      relayer.kill('SIGTERM');
      client.terminate();
      process.exit(1);
    } else if (result === Result.Continue) {
      // all good.
    } else if (result === Result.CleanExit) {
      console.log('Transaction Done and Relayed Successfully!');
      console.log(`Checking balance of the recipient (${recipient})`);
      // check the recipient balance
      const balance = await provider.getBalance(recipient);
      // the balance should be 1 ETH
      chai.assert(balance.eq(ethers.utils.parseEther('1')));
      console.log(`Balance equal to ${ethers.utils.formatEther(balance)} ETH`);
      console.log('Clean Exit');
      relayer.kill('SIGTERM');
      client.close();
      process.exit(0);
    } else {
      // ??
    }
  });
  client.on('error', (err) => {
    console.log('[E]', err);
    relayer.kill('SIGTERM');
    client.terminate();
    process.exit(1);
  });
  console.log('Generating zkProof to do a withdraw ..');
  const { proof, args } = await generateSnarkProof(
    depositArgs,
    recipient,
    contractAddress
  );
  console.log('Proof Generated!');
  const req = {
    evm: {
      ganache: {
        relayWithdrew: {
          contract: contractAddress,
          proof,
          root: args[0],
          nullifierHash: args[1],
          recipient: args[2],
          relayer: args[3],
          fee: args[4],
          refund: args[5],
        },
      },
    },
  };
  if (client.readyState === client.OPEN) {
    let data = JSON.stringify(req);
    console.log('Sending Proof to the Relayer ..');
    console.log('=>', data);
    client.send(data, (err) => {
      console.log('Proof Sent!');
      if (err !== undefined) {
        console.log('!!Error!!', err);
        relayer.kill('SIGTERM');
        client.terminate();
        process.exit(1);
      }
    });
    await sleep(45_000);
  } else {
    console.error('Relayer Connection closed!');
    relayer.kill();
    process.exit(1);
  }

  relayer.kill('SIGTERM');
  process.exit();
}

main().catch(console.error);
