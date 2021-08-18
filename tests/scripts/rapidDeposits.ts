import { ethers } from 'ethers';
require('dotenv').config({ path: '.env' });
import { 
  toHex,
  deposit,
  Deposit
} from '../proofUtils';

type configuredChain = {
  name: string,
  contractAddress: string,
  wallet: ethers.Signer,
  deposits: Deposit[],
}

function populateConfiguredChains(): configuredChain[] {

  let configuredChains: configuredChain[] = [];

  if (process.env.BERESHEET_ENDPOINT) {
    configuredChains.push({
      wallet: initializeWallet('beresheet'),
      name: 'beresheet',
      contractAddress: '0xf0EA8Fa17daCF79434d10C51941D8Fc24515AbE3',
      deposits: []
    });
  }
  
  if (process.env.HARMONY_ENDPOINT) {
    configuredChains.push({
      wallet: initializeWallet('harmony'),
      name: 'harmony',
      contractAddress: '0x4c37863bf2642Ba4e8De7e746500C700540119E8',
      deposits: []
    });
  }

  if (process.env.RINKEBY_ENDPOINT) {
    configuredChains.push({
      wallet: initializeWallet('rinkeby'),
      name: 'rinkeby',
      contractAddress: '0x626FEc5Ffa7Bf1EE8CEd7daBdE545630473E3ABb',
      deposits: []
    });
  }

  return configuredChains;
}

function initializeWallet(supportedChain: string): ethers.Signer {

  // A private key needs to be configured with the testnet funds
  if (!process.env.PRIVATE_KEY) {
    process.env.PRIVATE_KEY = "0x000000000000000000000000000000000000000000000000000000000000dead";
  }

  let provider: ethers.providers.JsonRpcProvider;
  let wallet: ethers.Signer;

  if (supportedChain == 'beresheet') {
    provider = new ethers.providers.JsonRpcProvider(process.env.BERESHEET_ENDPOINT);
    wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);
    return wallet;
  }
  else if (supportedChain == 'harmony') {
    provider = new ethers.providers.JsonRpcProvider(process.env.HARMONY_ENDPOINT);
    wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);
    return wallet;
  }
  else if (supportedChain == 'rinkeby') {
    provider = new ethers.providers.JsonRpcProvider(process.env.RINKEBY_ENDPOINT);
    wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);
    return wallet;
  }
  else {
    provider = new ethers.providers.JsonRpcProvider('http://localhost:9955');
    wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);
    return wallet;
  }
}

function generateNoteString(deposit: Deposit, chain: configuredChain): string {
  return `${chain.name}-${toHex(deposit.preimage, 62)}`
}

async function createDeposits() {  
  const configuredChains = populateConfiguredChains();

  setInterval(async () => {
    const res = await deposit(configuredChains[0]!.contractAddress, configuredChains[0]!.wallet);
    const noteString = generateNoteString(res, configuredChains[0]!);
    console.log(`made a deposit with: ${noteString}`);
  }, 10000);
};

createDeposits();
