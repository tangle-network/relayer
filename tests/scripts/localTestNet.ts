// This a simple script to start two local testnet chains and deploy the contracts on both of them

import readline from 'readline';
import { ethers } from 'ethers';
import { GanacheAccounts, startGanacheServer } from '../startGanacheServer';
import { Bridge } from '@webb-tools/fixed-bridge';
import { MintableToken } from '@webb-tools/tokens';
import { fetchComponentsFromFilePaths } from '@webb-tools/utils';
import path from 'path';

// Let's first define a localchain
class LocalChain {
  public readonly endpoint: string;
  private readonly server: any;
  constructor(
    public readonly name: string,
    public readonly chainId: number,
    readonly initalBalances: GanacheAccounts[]
  ) {
    this.endpoint = `http://localhost:${chainId}`;
    this.server = startGanacheServer(chainId, chainId, initalBalances);
  }

  public provider(): ethers.providers.WebSocketProvider {
    return new ethers.providers.WebSocketProvider(this.endpoint);
  }

  public async stop() {
    this.server.close();
  }

  public async deployToken(
    name: string,
    symbol: string,
    wallet: ethers.Signer
  ): Promise<MintableToken> {
    return MintableToken.createToken(name, symbol, wallet);
  }

  public async deployBridge(
    otherChain: LocalChain,
    localToken: MintableToken,
    otherToken: MintableToken,
    localWallet: ethers.Signer,
    otherWallet: ethers.Signer
  ): Promise<Bridge> {
    localWallet.connect(this.provider());
    otherWallet.connect(otherChain.provider());
    const bridgeInput = {
      anchorInputs: {
        asset: {
          [this.chainId]: [localToken.contract.address],
          [otherChain.chainId]: [otherToken.contract.address],
        },
        anchorSizes: [ethers.utils.parseEther('1')],
      },
      chainIDs: [this.chainId, otherChain.chainId],
    };
    const deployerConfig = {
      [this.chainId]: localWallet,
      [otherChain.chainId]: otherWallet,
    };
    const zkComponents = await fetchComponentsFromFilePaths(
      path.resolve(
        __dirname,
        '../protocol-solidity-fixtures/fixtures/bridge/2/poseidon_bridge_2.wasm'
      ),
      path.resolve(
        __dirname,
        '../protocol-solidity-fixtures/fixtures/bridge/2/witness_calculator.js'
      ),
      path.resolve(
        __dirname,
        '../protocol-solidity-fixtures/fixtures/bridge/2/circuit_final.zkey'
      )
    );

    return Bridge.deployBridge(bridgeInput, deployerConfig, zkComponents);
  }
}

async function main() {
  const relayerPrivateKey =
    '0x0000000000000000000000000000000000000000000000000000000000000001';
  const senderPrivateKey =
    '0x0000000000000000000000000000000000000000000000000000000000000002';

  const chainA = new LocalChain('Hermes', 5001, [
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: relayerPrivateKey,
    },
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: senderPrivateKey,
    },
  ]);
  const chainB = new LocalChain('Athena', 5002, [
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: relayerPrivateKey,
    },
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: senderPrivateKey,
    },
  ]);
  const chainAWallet = new ethers.Wallet(relayerPrivateKey, chainA.provider());
  const chainBWallet = new ethers.Wallet(relayerPrivateKey, chainB.provider());
  // do a random transfer on chainA to a random address
  // se we do have different nonce for that account.
  await chainAWallet.sendTransaction({
    to: '0x0000000000000000000000000000000000000000',
    value: ethers.utils.parseEther('0.001'),
  });
  // Deploy the token on chainA
  const chainAToken = await chainA.deployToken('ChainA', 'webbA', chainAWallet);
  // Deploy the token on chainB
  const chainBToken = await chainB.deployToken('ChainB', 'webbB', chainBWallet);
  // Deploy the bridge on one of the chain, will do deploy on the other too
  const bridge = await chainA.deployBridge(
    chainB,
    chainAToken,
    chainBToken,
    chainAWallet,
    chainBWallet
  );
  // get chainA bridge
  const chainABridge = bridge.getBridgeSide(chainA.chainId);
  // get chainB bridge
  const chainBBridge = bridge.getBridgeSide(chainB.chainId);
  // get the anhor on chainA
  const chainAAnchor = bridge.getAnchor(
    chainA.chainId,
    ethers.utils.parseEther('1')
  );
  await chainAAnchor.setSigner(chainAWallet);
  // get the anchor on chainB
  const chainBAnchor = bridge.getAnchor(
    chainB.chainId,
    ethers.utils.parseEther('1')
  );
  await chainBAnchor.setSigner(chainBWallet);
  // approve token spending
  const webbATokenAddress = bridge.getWebbTokenAddress(chainA.chainId)!;
  const webbAToken = await MintableToken.tokenFromAddress(
    webbATokenAddress,
    chainAWallet
  );
  await webbAToken.approveSpending(chainAAnchor.contract.address);
  await webbAToken.mintTokens(
    chainAWallet.address,
    ethers.utils.parseEther('1000')
  );

  const webbBTokenAddress = bridge.getWebbTokenAddress(chainB.chainId)!;
  const webbBToken = await MintableToken.tokenFromAddress(
    webbBTokenAddress,
    chainBWallet
  );
  await webbBToken.approveSpending(chainBAnchor.contract.address);
  await webbBToken.mintTokens(
    chainBWallet.address,
    ethers.utils.parseEther('1000')
  );
  // stop the server on Ctrl+C or SIGINT singal
  process.on('SIGINT', () => {
    chainA.stop();
    chainB.stop();
  });

  console.log('Chain A (Hermes):', chainA.endpoint);
  console.log('Chain B (Athena):', chainB.endpoint);
  console.log('ChainA token (Hermes): ', chainAToken.contract.address);
  console.log('ChainB token (Athena): ', chainBToken.contract.address);
  console.log('ChainA bridge (Hermes): ', chainABridge.contract.address);
  console.log('ChainB bridge (Athena): ', chainBBridge.contract.address);
  console.log('ChainA anchor (Hermes): ', chainAAnchor.contract.address);
  console.log('ChainB anchor (Athena): ', chainBAnchor.contract.address);

  printAvailableCommands();

  // setup readline
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  rl.on('line', async (cmdRaw) => {
    const cmd = cmdRaw.trim();
    if (cmd === 'exit') {
      // shutdown the servers
      await chainA.stop();
      await chainB.stop();
      rl.close();
      return;
    }
    // check if cmd is deposit chainA
    if (cmd.startsWith('deposit on chain a')) {
      console.log('Depositing Chain A, please wait...');
      const deposit = await chainAAnchor.deposit(chainB.chainId);
      console.log('Deposit on chain A: ', deposit.deposit);
      return;
    }

    if (cmd.startsWith('deposit on chain b')) {
      console.log('Depositing Chain B, please wait...');
      const deposit = await chainBAnchor.deposit(chainA.chainId);
      console.log('Deposit on chain B: ', deposit.deposit);
      return;
    }

    if (cmd.match(/^spam chain a (\d+)$/)) {
      const txs = parseInt(cmd.match(/^spam chain a (\d+)$/)?.[1] ?? '1');
      console.log(`Spamming Chain A with ${txs} Tx, please wait...`);
      for (let i = 0; i < txs; i++) {
        const deposit = await chainAAnchor.deposit(chainB.chainId);
        console.log('Deposit on chain A: ', deposit.deposit);
      }
      return;
    }

    if (cmd.match(/^spam chain b (\d+)$/)) {
      const txs = parseInt(cmd.match(/^spam chain b (\d+)$/)?.[1] ?? '1');
      console.log(`Spamming Chain B with ${txs}, please wait...`);
      for (let i = 0; i < txs; i++) {
        const deposit = await chainBAnchor.deposit(chainA.chainId);
        console.log('Deposit on chain B: ', deposit.deposit);
      }
      return;
    }

    if (cmd.startsWith('root on chain a')) {
      console.log('Root on chain A, please wait...');
      const root = await chainAAnchor.contract.getLastRoot();
      const latestNeighborRoots =
        await chainAAnchor.contract.getLatestNeighborRoots();
      console.log('Root on chain A: ', root);
      console.log('Latest neighbor roots on chain A: ', latestNeighborRoots);
      return;
    }

    if (cmd.startsWith('root on chain b')) {
      console.log('Root on chain B, please wait...');
      const root = await chainBAnchor.contract.getLastRoot();
      const latestNeighborRoots =
        await chainBAnchor.contract.getLatestNeighborRoots();
      console.log('Root on chain B: ', root);
      console.log('Latest neighbor roots on chain B: ', latestNeighborRoots);
      return;
    }

    console.log('Unknown command: ', cmd);
    printAvailableCommands();
  });
}

function printAvailableCommands() {
  console.log('Available commands:');
  console.log('  deposit on chain a');
  console.log('  deposit on chain b');
  console.log('  root on chain a');
  console.log('  root on chain b');
  console.log('  spam chain a <txs>');
  console.log('  spam chain b <txs>');
  console.log('  exit');
}

main().catch(console.error);
