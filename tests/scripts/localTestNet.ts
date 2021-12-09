// This a simple script to start two local testnet chains and deploy the contracts on both of them

import { ethers } from 'ethers';
import { GanacheAccounts, startGanacheServer } from '../startGanacheServer';
import { fixedBridge, tokens, utils } from '@webb-tools/protocol-solidity';
import path from 'path';
const { Bridge } = fixedBridge;
const { MintableToken } = tokens;
const { fetchComponentsFromFilePaths } = utils;

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
  ): Promise<tokens.MintableToken> {
    wallet.connect(this.provider());
    return MintableToken.createToken(name, symbol, wallet);
  }

  public async deployBridge(
    otherChain: LocalChain,
    localToken: tokens.MintableToken,
    otherToken: tokens.MintableToken,
    localWallet: ethers.Signer,
    otherWallet: ethers.Signer
  ): Promise<fixedBridge.Bridge> {
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
  const chainA = new LocalChain('ChainA', 5001, [
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: relayerPrivateKey,
    },
  ]);
  const chainB = new LocalChain('ChainB', 5002, [
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: relayerPrivateKey,
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
  // get the anhor on chainA
  const chainAAnchor = bridge.getAnchor(
    chainA.chainId,
    ethers.utils.parseEther('1')
  );
  // get the anchor on chainB
  const chainBAnchor = bridge.getAnchor(
    chainB.chainId,
    ethers.utils.parseEther('1')
  );
  // stop the server on Ctrl+C or SIGINT singal
  process.on('SIGINT', () => {
    chainA.stop();
    chainB.stop();
  });

  console.log('Chain A:', chainA.endpoint);
  console.log('Chain B:', chainB.endpoint);
  console.log('ChainA token: ', chainAToken.contract.address);
  console.log('ChainB token: ', chainBToken.contract.address);
  console.log('ChainA anchor: ', chainAAnchor.contract.address);
  console.log('ChainB anchor: ', chainBAnchor.contract.address);
}

main().catch(console.error);
