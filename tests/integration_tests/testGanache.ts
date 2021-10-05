import { assert, expect } from 'chai';
import ganache from 'ganache-cli';
import { ethers } from 'ethers';
import WebSocket from 'ws';
import nativeAnchorContract from '../build/contracts/NativeAnchor.json';
import verifierContract from '../build/contracts/Verifier.json';
import hasherContract from '../build/Hasher.json';
import {
  getAnchorDenomination,
  deposit,
  generateSnarkProof,
  getDepositLeavesFromChain,
  calculateFee,
} from '../proofUtils';
import {
  generateAnchorWithdrawRequest,
  RelayerChainConfig,
  getRelayerConfig,
  sleep,
  handleMessage,
  Result,
  startWebbRelayer,
} from '../relayerUtils';
import { ChildProcessWithoutNullStreams } from 'child_process';

function startGanacheServer() {
  const ganacheServer = ganache.server({
    accounts: [
      {
        balance: ethers.utils.parseEther('1000').toHexString(),
        secretKey: PRIVATE_KEY,
      },
    ],
    port: PORT,
    mnemonic:
      'congress island collect purity dentist team gas unlock nuclear pig combine sight',
  });

  ganacheServer.listen(PORT);
  console.log(`Ganache Started on http://127.0.0.1:${PORT} ..`);
  return ganacheServer;
}

async function deployNativeAnchor(wallet: ethers.Wallet) {
  const hasherContractRaw = {
    contractName: 'Hasher',
    abi: hasherContract.abi,
    bytecode: hasherContract.bytecode,
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

const PRIVATE_KEY =
  '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7e';
const PORT = 8545;
const ENDPOINT = 'http://127.0.0.1:8545';
let ganacheServer: any;
let relayer: ChildProcessWithoutNullStreams;
let recipient: string;
let wallet: ethers.Wallet;
let provider: ethers.providers.Provider;
let contractAddress: string;
let contractDenomination: string;
let calculatedFee: string;
let startingRecipientBalance: ethers.BigNumber;
let proof: string;
let args: string[];
let client: WebSocket;
let relayerChainInfo: RelayerChainConfig;

describe('Ganache Relayer Withdraw Tests', function () {
  // increase the timeout for relayer tests
  this.timeout(30_000);

  before(async function () {
    ganacheServer = startGanacheServer();
    console.log('Starting the Relayer ..');
    relayer = await startWebbRelayer();
    await sleep(1500); // wait for the relayer start-up

    provider = new ethers.providers.WebSocketProvider(ENDPOINT);
    wallet = new ethers.Wallet(PRIVATE_KEY, provider);

    recipient = ethers.utils.getAddress(
      '0xe8f999AC5DAa08e134735016FAcE0D6439baFF94'
    );
    startingRecipientBalance = await provider.getBalance(recipient);

    contractAddress = await deployNativeAnchor(wallet);
    contractDenomination = await getAnchorDenomination(
      contractAddress,
      provider
    );

    // get the info from the relayer
    relayerChainInfo = await getRelayerConfig(
      'ganache',
      'http://localhost:9955'
    );
    console.log(JSON.stringify(relayerChainInfo));

    const contractConfig = relayerChainInfo.contracts.find(
      (e) => e.address.toLowerCase() === contractAddress.toLowerCase()
    );

    // save the relayer configured parameters
    calculatedFee = calculateFee(
      contractConfig?.withdrawFeePercentage ?? 0.0,
      contractDenomination
    );
  });

  describe('Sunny day setup', function () {
    before(async function () {
      // make a deposit
      const depositArgs = await deposit(contractAddress, wallet);

      // get the leaves
      const leaves = await getDepositLeavesFromChain(contractAddress, provider);

      // generate the withdraw tx to send to relayer
      const { proof: zkProof, args: zkArgs } = await generateSnarkProof(
        leaves,
        depositArgs,
        recipient,
        relayerChainInfo.account,
        calculatedFee
      );

      proof = zkProof;
      args = zkArgs;

      // setup relayer connections
      client = new WebSocket('ws://localhost:9955/ws');
      await new Promise((resolve) => client.on('open', resolve));
      console.log('Connected to Relayer!');
    });

    it('should work in sunny day scenario', function (done) {
      // Setup relayer interaction with logging
      client.on('message', async (data) => {
        console.log('Received data from the relayer');
        console.log('<==', data);
        const msg = JSON.parse(data as string);
        const result = handleMessage(msg);
        if (result === Result.Errored) {
          done('Relayer errored in sunny day');
        } else if (result === Result.Continue) {
          // all good.
          return;
        } else if (result === Result.CleanExit) {
          console.log('Transaction Done and Relayed Successfully!');
          // check the recipient balance
          const endingRecipientBalance = await provider.getBalance(recipient);
          //@ts-ignore
          const changeInBalance = contractDenomination - calculatedFee;
          assert(
            endingRecipientBalance.eq(
              //@ts-ignore
              startingRecipientBalance + changeInBalance
            )
          );
          done();
        }
      });
      client.on('error', (err) => {
        console.log('[E]', err);
        done('Client connection errored in sunny day');
      });

      const req = generateAnchorWithdrawRequest(
        'ganache',
        contractAddress,
        proof,
        args
      );
      if (client.readyState === client.OPEN) {
        const data = JSON.stringify(req);
        console.log('Sending Proof to the Relayer ..');
        console.log('=>', data);
        client.send(data, (err) => {
          console.log('Proof Sent!');
          if (err !== undefined) {
            console.log('!!Error!!', err);
            done('Client error sending proof');
          }
        });
      } else {
        console.error('Relayer Connection closed!');
        done('Client error, not OPEN');
      }
    });

    after(async function () {
      client.terminate();
    });
  });

  describe('invalid relayer address setup', function () {
    before(async function () {
      // make a deposit
      const depositArgs = await deposit(contractAddress, wallet);

      // get the leaves
      const leaves = await getDepositLeavesFromChain(contractAddress, provider);

      // generate the withdraw tx to send to relayer
      const { proof: zkProof, args: zkArgs } = await generateSnarkProof(
        leaves,
        depositArgs,
        recipient,
        recipient,
        calculatedFee
      );

      proof = zkProof;
      args = zkArgs;

      // setup relayer connections
      client = new WebSocket('ws://localhost:9955/ws');
      await new Promise((resolve) => client.on('open', resolve));
      console.log('Connected to Relayer!');
    });

    it('Should not send transaction with different relayer address', function (done) {
      // Setup relayer interaction with logging
      client.on('message', async (data) => {
        console.log('Received data from the relayer');
        console.log('<==', data);
        const msg = JSON.parse(data as string);
        const result = handleMessage(msg);
        if (result === Result.Errored) {
          // it should be errored.
          done();
        } else if (result === Result.Continue) {
          // all good.
          return;
        } else if (result === Result.CleanExit) {
          console.log('Transaction Done and Relayed Successfully!');
          done('Transaction was submitted and executed');
        }
      });
      client.on('error', (err) => {
        console.log('[E]', err);
        done('Client connection errored unexpectedly');
      });

      const req = generateAnchorWithdrawRequest(
        'ganache',
        contractAddress,
        proof,
        args
      );
      if (client.readyState === client.OPEN) {
        const data = JSON.stringify(req);
        console.log('Sending Proof to the Relayer ..');
        console.log('=>', data);
        client.send(data, (err) => {
          console.log('Proof Sent!');
          if (err !== undefined) {
            console.log('!!Error!!', err);
            done('Client error sending proof');
          }
        });
      } else {
        console.error('Relayer Connection closed!');
        done('Client error, not OPEN');
      }
    });

    after(function () {
      client.terminate();
    });
  });

  describe('invalid fee setup', function () {
    before(async function () {
      // make a deposit
      const depositArgs = await deposit(contractAddress, wallet);

      // get the leaves
      const leaves = await getDepositLeavesFromChain(contractAddress, provider);

      // generate the withdraw tx to send to relayer
      const { proof: zkProof, args: zkArgs } = await generateSnarkProof(
        leaves,
        depositArgs,
        recipient,
        relayerChainInfo.account,
        '0'
      );

      proof = zkProof;
      args = zkArgs;

      // setup relayer connections
      client = new WebSocket('ws://localhost:9955/ws');
      await new Promise((resolve) => client.on('open', resolve));
      console.log('Connected to Relayer!');
    });

    it('should not relay a transaction with a fee that is too low', function (done) {
      // Setup relayer interaction with logging
      client.on('message', async (data) => {
        console.log('Received data from the relayer');
        console.log('<==', data);
        const msg = JSON.parse(data as string);
        const result = handleMessage(msg);
        if (result === Result.Errored) {
          expect(msg).to.deep.equal({
            error:
              'User sent a fee that is too low 0 but expected 50000000000000000',
          });
          done();
        } else if (result === Result.Continue) {
          // all good.
          return;
        } else if (result === Result.CleanExit) {
          console.log('Transaction Done and Relayed Successfully!');
          done('Transaction was submitted and executed');
        }
      });
      client.on('error', (err) => {
        console.log('[E]', err);
        done('Client connection errored unexpectedly');
      });

      const req = generateAnchorWithdrawRequest(
        'ganache',
        contractAddress,
        proof,
        args
      );
      if (client.readyState === client.OPEN) {
        const data = JSON.stringify(req);
        console.log('Sending Proof to the Relayer ..');
        console.log('=>', data);
        client.send(data, (err) => {
          console.log('Proof Sent!');
          if (err !== undefined) {
            console.log('!!Error!!', err);
            done('Client error sending proof');
          }
        });
      } else {
        console.error('Relayer Connection closed!');
        done('Client error, not OPEN');
      }
    });

    after(function () {
      client.terminate();
    });
  });

  describe('bad proof (missing correct relayer address)', function () {
    before(async function () {
      // make a deposit
      const depositArgs = await deposit(contractAddress, wallet);

      // get the leaves
      const leaves = await getDepositLeavesFromChain(contractAddress, provider);

      // generate the withdraw tx to send to relayer
      const { proof: zkProof, args: zkArgs } = await generateSnarkProof(
        leaves,
        depositArgs,
        recipient,
        recipient, // relayer
        calculatedFee
      );

      proof = zkProof;
      args = zkArgs;
      args[3] = relayerChainInfo.account;
      console.log(args);

      // setup relayer connections
      client = new WebSocket('ws://localhost:9955/ws');
      await new Promise((resolve) => client.on('open', resolve));
      console.log('Connected to Relayer!');
    });

    it('should not relay a transaction with a bad proof', function (done) {
      // Setup relayer interaction with logging
      client.on('message', async (data) => {
        console.log('Received data from the relayer');
        console.log('<==', data);
        const msg = JSON.parse(data as string);
        const result = handleMessage(msg);
        if (result === Result.Errored) {
          expect(msg).to.deep.equal({
            withdraw: {
              errored: { code: -32000, reason: 'Invalid withdraw proof' },
            },
          });
          done();
        } else if (result === Result.Continue) {
          // all good.
          return;
        } else if (result === Result.CleanExit) {
          console.log('Transaction Done and Relayed Successfully!!!');
          expect(
            false,
            'Transaction was submitted and executed, which should not have happened!'
          );
          done();
        }
      });
      client.on('error', (err) => {
        console.log('[E]', err);
        done('Client connection errored unexpectedly');
      });

      const req = generateAnchorWithdrawRequest(
        'ganache',
        contractAddress,
        proof,
        args
      );
      if (client.readyState === client.OPEN) {
        const data = JSON.stringify(req);
        console.log('Sending Proof to the Relayer ..');
        console.log('=>', data);
        client.send(data, (err) => {
          console.log('Proof Sent!');
          if (err !== undefined) {
            console.log('!!Error!!', err);
            done('Client error sending proof');
          }
        });
      } else {
        console.error('Relayer Connection closed!');
        done('Client error, not OPEN');
      }
    });

    after(function () {
      client.terminate();
    });
  });

  after(function () {
    ganacheServer.close(console.error);
    client.terminate();
    relayer.kill('SIGINT');
  });
});
