import { assert } from 'chai';
import { ethers } from 'ethers';
import WebSocket from 'ws';
import { Bridge, Anchor } from '@webb-tools/fixed-bridge';
import { MintableToken } from '@webb-tools/tokens';
import { fetchComponentsFromFilePaths, toFixedHex } from '@webb-tools/utils';
import {
  RelayerChainConfig,
  sleep,
  handleMessage,
  Result,
  startWebbRelayer,
  generateAnchorWithdrawRequest,
  getRelayerConfig,
} from '../../relayerUtils';
import { startGanacheServer } from '../../startGanacheServer';
import { ChildProcessWithoutNullStreams } from 'child_process';
import path from 'path';

// deployer and relayer same private key so proposals can be voted and executed
let relayerPrivateKey =
  '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7e';
let deployerPrivateKey =
  '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7e';
let senderAddress = '0x42f620334F6415BB437C1c041DA24653A073405b';
let senderPrivateKey =
  '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d72';

let chainId1 = 3333;
let ganacheServer1: any;
let provider1: ethers.providers.Provider;

let chainId2 = 4444;
let ganacheServer2: any;
let provider2: ethers.providers.Provider;

// variables intended to be consistent across all tests
let relayer: ChildProcessWithoutNullStreams;
let relayerEndpoint: string;
let recipient: string;
let bridge: Bridge;
let relayerChain1Info: RelayerChainConfig;
let relayerChain2Info: RelayerChainConfig;

// Variables intended to change across each test
let sourceWallet: ethers.Wallet;
let destWallet: ethers.Wallet;
let anchorContractAddress: string;
let proof: string;
let args: string[];
let client: WebSocket;
let startingRecipientBalance: ethers.BigNumber;

describe('Anchor Tests', function () {
  // increase the timeout for relayer tests
  this.timeout(120_000);

  before(async function () {
    // Ganache setup accounts and servers
    let ganacheAccounts = [
      {
        balance: ethers.utils.parseEther('1000').toHexString(),
        secretKey: deployerPrivateKey,
      },
      {
        balance: ethers.utils.parseEther('1000').toHexString(),
        secretKey: relayerPrivateKey,
      },
      {
        balance: ethers.utils.parseEther('1000').toHexString(),
        secretKey: senderPrivateKey,
      },
    ];

    ganacheServer1 = startGanacheServer(3333, chainId1, ganacheAccounts);
    provider1 = new ethers.providers.WebSocketProvider('http://localhost:3333');

    ganacheServer2 = startGanacheServer(4444, chainId2, ganacheAccounts);
    provider2 = new ethers.providers.WebSocketProvider('http://localhost:4444');

    // Deploy token contracts
    const wallet1 = new ethers.Wallet(deployerPrivateKey, provider1);
    const wallet2 = new ethers.Wallet(deployerPrivateKey, provider2);

    const tokenInstance1 = await MintableToken.createToken(
      'testToken',
      'tTKN',
      wallet1
    );
    await tokenInstance1.mintTokens(senderAddress, '100000000000000000000');
    const tokenInstance2 = await MintableToken.createToken(
      'testToken',
      'tTKN',
      wallet2
    );

    console.log('finished token deployments');

    // Deploy the bridge
    const bridgeInput = {
      anchorInputs: {
        asset: {
          [chainId1]: [tokenInstance1.contract.address],
          [chainId2]: [tokenInstance2.contract.address],
        },
        anchorSizes: ['1000000000000000000'],
      },
      chainIDs: [chainId1, chainId2],
    };
    const deployerConfig = {
      [chainId1]: wallet1,
      [chainId2]: wallet2,
    };
    const zkComponents = await fetchComponentsFromFilePaths(
      path.resolve(
        __dirname,
        '../../protocol-solidity-fixtures/fixtures/bridge/2/poseidon_bridge_2.wasm'
      ),
      path.resolve(
        __dirname,
        '../../protocol-solidity-fixtures/fixtures/bridge/2/witness_calculator.js'
      ),
      path.resolve(
        __dirname,
        '../../protocol-solidity-fixtures/fixtures/bridge/2/circuit_final.zkey'
      )
    );

    bridge = await Bridge.deployBridge(
      bridgeInput,
      deployerConfig,
      zkComponents
    );

    console.log('finished bridge deployments');

    // Mint tokens to the sender
    const webbTokenAddress1 = bridge.getWebbTokenAddress(chainId1)!;
    const webbToken1 = await MintableToken.tokenFromAddress(
      webbTokenAddress1,
      wallet1
    );
    await webbToken1.mintTokens(senderAddress, '1000000000000000000000');
    const webbTokenAddress2 = bridge.getWebbTokenAddress(chainId2)!;
    const webbToken2 = await MintableToken.tokenFromAddress(
      webbTokenAddress2,
      wallet2
    );
    await webbToken2.mintTokens(senderAddress, '1000000000000000000000');

    // Setup webb relayer
    console.log('Starting the Relayer ..');
    [relayer, relayerEndpoint] = await startWebbRelayer(8888);
    await sleep(1500); // wait for the relayer start-up

    relayerChain1Info = await getRelayerConfig('testa', relayerEndpoint);
    relayerChain2Info = await getRelayerConfig('testb', relayerEndpoint);
    recipient = '0xe8f999AC5DAa08e134735016FAcE0D6439baFF94';
  });

  describe('Sunny day Anchor withdraw relayed transaction same chain', function () {
    before(async function () {
      this.timeout(120_000);

      sourceWallet = new ethers.Wallet(senderPrivateKey, provider1);
      const srcAnchor = await bridge.getAnchor(chainId1, '1000000000000000000');
      await srcAnchor.setSigner(sourceWallet);
      anchorContractAddress = srcAnchor.contract.address;

      // approve token spending
      const webbTokenAddress = await bridge.getWebbTokenAddress(chainId1)!;
      const webbToken = await MintableToken.tokenFromAddress(
        webbTokenAddress,
        sourceWallet
      );
      await webbToken.approveSpending(srcAnchor.contract.address);
      startingRecipientBalance = await webbToken.getBalance(recipient);

      // deposit
      const deposit = await srcAnchor.deposit(chainId1);

      // allow time for the bridge proposal and execution
      console.log('waiting for bridge proposal and execution');
      await sleep(20_000);

      // generate the withdraw
      const withdrawInfo = await srcAnchor.setupWithdraw(
        deposit.deposit,
        deposit.index,
        recipient,
        relayerChain1Info.beneficiary,
        BigInt(0),
        0
      );

      proof = `0x${withdrawInfo.proofEncoded}`;
      args = withdrawInfo.args;

      console.log(`proof: ${proof} \n args: ${args}`);

      // setup relayer connections

      client = new WebSocket(`${relayerEndpoint.replace('http', 'ws')}/ws`);
      await new Promise((resolve) => client.on('open', resolve));
      console.log('Connected to Relayer!');
    });

    it('should relay successfully', function (done) {
      this.timeout(120_000);
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
          const webbTokenAddress = await bridge.getWebbTokenAddress(chainId1)!;
          const webbToken = await MintableToken.tokenFromAddress(
            webbTokenAddress,
            sourceWallet
          );
          const endingRecipientBalance = await webbToken.getBalance(recipient);
          const changeInBalance: ethers.BigNumberish = '1000000000000000000';
          assert(
            endingRecipientBalance.eq(
              startingRecipientBalance.add(changeInBalance)
            )
          );
          done();
        }
      });
      client.on('error', (err) => {
        console.log('[E]', err);
        done('Client connection errored unexpectedly');
      });

      const req = generateAnchorWithdrawRequest(
        'testa',
        anchorContractAddress,
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
      console.log('terminating websockets connection');
      client.terminate();
      await sleep(2000);
    });
  });

  describe.only('Sunny day Anchor withdraw relayed transaction across bridge', function () {
    this.timeout(120_000);
    before(async function () {
      this.timeout(120_000);
      sourceWallet = new ethers.Wallet(senderPrivateKey, provider1);
      destWallet = new ethers.Wallet(senderPrivateKey, provider2);
      const chainABridge = bridge.getBridgeSide(chainId1);
      const chainBBridge = bridge.getBridgeSide(chainId2);
      console.log('chainABridge: ', chainABridge.contract.address);
      console.log('chainBBridge: ', chainBBridge.contract.address);
      const srcAnchor = await bridge.getAnchor(chainId1, '1000000000000000000');
      console.log('Chain A Anchor Address: ', srcAnchor.contract.address);
      await srcAnchor.setSigner(sourceWallet);

      // approve token spending
      const webbToken1Address = await bridge.getWebbTokenAddress(chainId1)!;
      const webbToken1 = await MintableToken.tokenFromAddress(
        webbToken1Address,
        sourceWallet
      );
      await webbToken1.approveSpending(srcAnchor.contract.address);

      const webbToken2Address = await bridge.getWebbTokenAddress(chainId2)!;
      const webbToken2 = await MintableToken.tokenFromAddress(
        webbToken2Address,
        destWallet
      );
      startingRecipientBalance = await webbToken2.getBalance(recipient);

      // deposit
      console.log('before the first deposit');
      await srcAnchor.deposit(chainId2);

      // allow time for the bridge proposal and execution
      console.log('waiting for bridge proposal and execution');
      await sleep(20_000);

      console.log('before the second deposit');
      const deposit = await srcAnchor.deposit(chainId2);
      console.log('waiting for another bridge proposal and execution');
      await sleep(20_000);

      console.log('before the third deposit');
      await srcAnchor.deposit(chainId2);
      console.log('waiting for another bridge proposal and execution');
      await sleep(20_000);

      // generate the merkle proof from the source anchor
      await srcAnchor.checkKnownRoot();
      const { pathElements, pathIndices } = srcAnchor.tree.path(deposit.index);

      // update the destAnchor
      const destAnchor = await bridge.getAnchor(
        chainId2,
        '1000000000000000000'
      );
      console.log('Chain B Anchor Address: ', destAnchor.contract.address);
      anchorContractAddress = destAnchor.contract.address;

      const destRoots = await destAnchor.populateRootsForProof();

      const input = await destAnchor.generateWitnessInput(
        deposit.deposit,
        deposit.originChainId,
        '0x0000000000000000000000000000000000000000000000000000000000000000',
        BigInt(recipient),
        BigInt(relayerChain2Info.beneficiary),
        BigInt(0),
        BigInt(0),
        destRoots,
        pathElements,
        pathIndices
      );
      const wtns = await destAnchor.createWitness(input);
      const proofEncoded = await destAnchor.proveAndVerify(wtns);
      const proofArgs = [
        Anchor.createRootsBytes(input.roots),
        toFixedHex(input.nullifierHash),
        toFixedHex(input.refreshCommitment, 32),
        toFixedHex(input.recipient, 20),
        toFixedHex(input.relayer, 20),
        toFixedHex(input.fee),
        toFixedHex(input.refund),
      ];

      proof = `0x${proofEncoded}`;
      args = proofArgs;

      // setup relayer connections
      client = new WebSocket(`${relayerEndpoint.replace('http', 'ws')}/ws`);
      await new Promise((resolve) => client.on('open', resolve));
      console.log('Connected to Relayer!');
    });

    it('should relay successfully', function (done) {
      this.timeout(120_000);
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
          const webbTokenAddress = await bridge.getWebbTokenAddress(chainId2)!;
          const webbToken = await MintableToken.tokenFromAddress(
            webbTokenAddress,
            destWallet
          );
          const endingRecipientBalance = await webbToken.getBalance(recipient);
          const changeInBalance: ethers.BigNumberish = '1000000000000000000';
          assert(
            endingRecipientBalance.eq(
              startingRecipientBalance.add(changeInBalance)
            )
          );
          done();
        }
      });
      client.on('error', (err) => {
        console.log('[E]', err);
        done('Client connection errored unexpectedly');
      });

      const req = generateAnchorWithdrawRequest(
        'testb',
        anchorContractAddress.toLowerCase(),
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
      client?.terminate();
    });
  });

  after(function () {
    ganacheServer1.close(console.error);
    ganacheServer2.close(console.error);
    client?.terminate();
    relayer.kill('SIGINT');
  });
});
