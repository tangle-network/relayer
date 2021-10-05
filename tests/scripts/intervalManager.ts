import { ethers } from 'ethers';
require('dotenv').config({ path: '.env' });
import { create_slack_alert } from './dispatchSlackNotification';
import WebSocket from 'ws';
import {
  generateSnarkProof,
  getDepositLeavesFromRelayer,
  getAnchorDenomination,
  calculateFee,
  toHex,
  deposit,
  Deposit,
} from '../proofUtils';
import {
  getRelayerConfig,
  generateAnchorWithdrawRequest,
  handleMessage,
  Result,
  sleep,
} from '../relayerUtils';

type configuredChain = {
  name: string;
  contractAddress: string;
  wallet: ethers.Signer;
};

function populateConfiguredChains(): configuredChain[] {
  let configuredChains: configuredChain[] = [];

  if (process.env.BERESHEET_ENDPOINT) {
    configuredChains.push({
      wallet: initializeWallet('beresheet'),
      name: 'beresheet',
      contractAddress: '0xf0EA8Fa17daCF79434d10C51941D8Fc24515AbE3',
    });
  }

  if (process.env.HARMONY_ENDPOINT) {
    configuredChains.push({
      wallet: initializeWallet('harmony'),
      name: 'harmony',
      contractAddress: '0x4c37863bf2642Ba4e8De7e746500C700540119E8',
    });
  }

  if (process.env.RINKEBY_ENDPOINT) {
    configuredChains.push({
      wallet: initializeWallet('rinkeby'),
      name: 'rinkeby',
      contractAddress: '0x626FEc5Ffa7Bf1EE8CEd7daBdE545630473E3ABb',
    });
  }

  return configuredChains;
}

function initializeWallet(supportedChain: string): ethers.Signer {
  // A private key needs to be configured with the testnet funds
  if (!process.env.PRIVATE_KEY) {
    process.env.PRIVATE_KEY =
      '0x000000000000000000000000000000000000000000000000000000000000dead';
  }

  let provider: ethers.providers.JsonRpcProvider;
  let wallet: ethers.Signer;

  if (supportedChain == 'beresheet') {
    provider = new ethers.providers.JsonRpcProvider(
      process.env.BERESHEET_ENDPOINT
    );
    wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);
    return wallet;
  } else if (supportedChain == 'harmony') {
    provider = new ethers.providers.JsonRpcProvider(
      process.env.HARMONY_ENDPOINT
    );
    wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);
    return wallet;
  } else if (supportedChain == 'rinkeby') {
    provider = new ethers.providers.JsonRpcProvider(
      process.env.RINKEBY_ENDPOINT
    );
    wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);
    return wallet;
  } else {
    provider = new ethers.providers.JsonRpcProvider('http://localhost:9955');
    wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);
    return wallet;
  }
}

function generateNoteString(deposit: Deposit, chain: configuredChain): string {
  return `${chain.name}-${toHex(deposit.preimage, 62)}`;
}

async function run() {
  setInterval(async () => {
    const configuredChains = populateConfiguredChains();

    for (let i = 0; i < configuredChains.length; i++) {
      const res = await deposit(
        configuredChains[i]!.contractAddress,
        configuredChains[i]!.wallet
      );
      const noteString = generateNoteString(res, configuredChains[i]!);
      console.log(`made a deposit with: ${noteString}`);

      // allow time for relayer polling to see deposit
      await sleep(30000);

      const relayerInfo = await getRelayerConfig(
        configuredChains[i]!.name,
        `${process.env.RELAYER_ENDPOINT_HTTP}`
      );
      const contractDenomination = await getAnchorDenomination(
        configuredChains[i]!.contractAddress,
        configuredChains[i]!.wallet.provider!
      );
      const calculatedFee = calculateFee(
        relayerInfo.withdrawFeePercentage,
        contractDenomination
      );

      const client = new WebSocket(
        process.env.RELAYER_ENDPOINT_WS || 'ws://localhost:9955/ws'
      );
      await new Promise((resolve) => client.on('open', resolve));

      // Setup websockets response to data
      client.on('message', async (data) => {
        console.log('<==', data);
        const msg = JSON.parse(data as string);
        const result = await handleMessage(msg);
        if (result === Result.Errored) {
          client.terminate();
          create_slack_alert('Relayed withdraw failed.', `Error: ${msg}`);
        } else if (result === Result.Continue) {
          // all good.
          return;
        } else if (result === Result.CleanExit) {
          console.log('Transaction Done and Relayed Successfully!');
          console.log('Clean Exit');
          client.close();
        } else {
          // ??
        }
      });
      client.on('error', (err) => {
        console.log('[E]', err);
        client.terminate();
        create_slack_alert('Websockets communication failed.', `Error: ${err}`);
      });

      console.log('Generating zkProof to do a withdraw ..');
      const leaves = await getDepositLeavesFromRelayer(
        configuredChains[i]!.contractAddress,
        `${process.env.RELAYER_ENDPOINT_HTTP}`
      );
      const { proof, args } = await generateSnarkProof(
        leaves,
        res,
        await configuredChains[i]!.wallet.getAddress(),
        relayerInfo.account,
        calculatedFee
      );
      console.log('Proof Generated!');
      const req = generateAnchorWithdrawRequest(
        configuredChains[i]!.name,
        configuredChains[i]!.contractAddress,
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
            client.terminate();
            create_slack_alert(
              'Websockets sending proof failed.',
              `Error: ${err}`
            );
          }
        });
      } else {
        client.terminate();
        console.error('Relayer Connection closed!');
        create_slack_alert('Websockets client was not in open state');
      }
    }
  }, 600000); // run every 10 minutes
}

run();
