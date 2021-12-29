import { assert, expect } from 'chai';
import { ethers } from 'ethers';
import WebSocket from 'ws';
import {
  generateSubstrateMixerSnarkProof,
  getDepositLeavesFromSubstrateChain,
  calculateFee,
  withdrawSubstrateMixer,
  depositSubstrateMixer,
} from '../../proofUtils';
import {
  RelayerChainConfig,
  sleep,
  handleMessage,
  Result,
  startWebbRelayer,
  generateSubstrateWithdrawRequest,
} from '../../relayerUtils';
import { ChildProcessWithoutNullStreams } from 'child_process';
import fetch from 'node-fetch';
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { options } from '@webb-tools/api';


let relayer: ChildProcessWithoutNullStreams;
let recipient: string;
let wallet: ethers.Wallet;
let provider: ethers.providers.Provider;
let tornadoContractAddress: string;
let contractDenomination: string;
let calculatedFee: string;
let startingRecipientBalance: ethers.BigNumber;
let proof: string;
let args: string[];
let client: WebSocket;
let relayerChainInfo: RelayerChainConfig;
const keyring = new Keyring({ type: "sr25519" });
const alice = keyring.addFromUri("//Alice");

describe('Substrate Mixer Relayer Withdraw Tests', function () {
  let api;
  const treeId = '0';
  // increase the timeout for relayer tests
  this.timeout(30_000);

  before(async function () {
    const provider = new WsProvider('ws://127.0.0.1:9944');
    api = new ApiPromise(options({ provider }));
    await api.isReady;
    console.log('Starting the Relayer ..');
    relayer = await startWebbRelayer();
    await sleep(1500); // wait for the relayer start-up

    // get the info from the relayer
    const endpoint = 'http://localhost:9955';
    const relayerInfoRes: any = await (await fetch(`${endpoint}/api/v1/info`)).json();
    console.log(relayerInfoRes);

    const contractConfig = relayerInfoRes.contracts.find(
      (e: any) => e.address.toLowerCase() === tornadoContractAddress.toLowerCase()
    );

    // save the relayer configured parameters
    calculatedFee = calculateFee(
      contractConfig?.withdrawFeePercentage ?? 0.0,
      contractDenomination
    );
  });

  describe('Sunny day Mixer Relayed transaction', function () {
    before(async function () {
      // make a deposit
      const depositArgs = await depositSubstrateMixer(treeId);

      // get the leaves
      const leaves = await getDepositLeavesFromSubstrateChain(treeId);

      // generate the withdraw tx to send to relayer
      const { proof: zkProof, args: zkArgs } = await generateSubstrateMixerSnarkProof(
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

      const treeId = '0';
      const req = generateSubstrateWithdrawRequest(
        'substrate',
        treeId,
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
      const depositArgs = await depositSubstrateMixer(treeId);

      // get the leaves
      const leaves = await getDepositLeavesFromSubstrateChain(treeId);

      // generate the withdraw tx to send to relayer
      const { proof: zkProof, args: zkArgs } = await generateSubstrateMixerSnarkProof(
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

      const req = generateSubstrateWithdrawRequest(
        'ganache',
        tornadoContractAddress,
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
      const depositArgs = await depositSubstrateMixer(treeId);

      // get the leaves
      const leaves = await getDepositLeavesFromSubstrateChain(treeId);

      // generate the withdraw tx to send to relayer
      const { proof: zkProof, args: zkArgs } = await generateSubstrateMixerSnarkProof(
        leaves,
        depositArgs,
        recipient,
        relayerChainInfo.beneficiary,
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

      const req = generateSubstrateWithdrawRequest(
        'ganache',
        tornadoContractAddress,
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
      const depositArgs = await depositSubstrateMixer(treeId);

      // get the leaves
      const leaves = await getDepositLeavesFromSubstrateChain(treeId);

      // generate the withdraw tx to send to relayer
      const { proof: zkProof, args: zkArgs } = await generateSubstrateMixerSnarkProof(
        leaves,
        depositArgs,
        recipient,
        recipient, // relayer
        calculatedFee
      );

      proof = zkProof;
      args = zkArgs;
      args[3] = relayerChainInfo.beneficiary;
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
              errored: { code: -32000, reason: 'VM Exception while processing transaction: revert Invalid withdraw proof' },
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

      const req = generateSubstrateWithdrawRequest(
        'ganache',
        tornadoContractAddress,
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

  describe('Already spent note', function () {
    before(async function () {
      // make a deposit
      const depositArgs = await depositSubstrateMixer(treeId);

      // get the leaves
      const leaves = await getDepositLeavesFromSubstrateChain(treeId);
      
      // generate the withdraw tx to send to relayer
      const { proof: zkProof, args: zkArgs } = await generateSubstrateMixerSnarkProof(
        leaves,
        depositArgs,
        recipient,
        relayerChainInfo.beneficiary, // relayer
        calculatedFee
      );

      proof = zkProof;
      args = zkArgs;
      args[3] = relayerChainInfo.beneficiary;
      console.log(args);

      // withdraw the deposit
      await withdrawSubstrateMixer(treeId, proof, args);

      // setup relayer connections
      client = new WebSocket('ws://localhost:9955/ws');
      await new Promise((resolve) => client.on('open', resolve));
      console.log('Connected to Relayer!');
    });

    it('should not relay a transaction from an already spent note', function (done) {
      // Setup relayer interaction with logging
      client.on('message', async (data) => {
        console.log('Received data from the relayer');
        console.log('<==', data);
        const msg = JSON.parse(data as string);
        const result = handleMessage(msg);
        if (result === Result.Errored) {
          expect(msg).to.deep.equal({
            withdraw: {
              errored: { code: -32000, reason: 'VM Exception while processing transaction: revert The note has been already spent' },
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

      const req = generateSubstrateWithdrawRequest(
        'substrate',
        '0',
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
    client.terminate();
    relayer.kill('SIGINT');
  });
});
