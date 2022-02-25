import fs from 'fs';
import ganache from 'ganache';
import { ethers } from 'ethers';
import { Server } from 'ganache';
import { Anchors, Bridges } from '@webb-tools/protocol-solidity';
import {
  BridgeInput,
  DeployerConfig,
  GovernorConfig,
} from '@webb-tools/interfaces';
import { MintableToken } from '@webb-tools/tokens';
import { fetchComponentsFromFilePaths } from '@webb-tools/utils';
import path from 'path';
import child from 'child_process';
import { ChainInfo, Contract, EventsWatcher } from './webbRelayer';

export type GanacheAccounts = {
  balance: string;
  secretKey: string;
};

export function startGanacheServer(
  port: number,
  networkId: number,
  populatedAccounts: GanacheAccounts[],
  options: any = {}
): Server<'ethereum'> {
  const ganacheServer = ganache.server({
    accounts: populatedAccounts,
    quiet: true,
    network_id: networkId,
    chainId: networkId,
    ...options,
  });

  ganacheServer.listen(port).then(() => {
    process.stdout.write(`Ganache Started on http://127.0.0.1:${port} ..\n`);
  });

  return ganacheServer;
}

export class LocalChain {
  public readonly endpoint: string;
  private readonly server: Server<'ethereum'>;
  private signatureBridge: Bridges.SignatureBridge | null = null;
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
    await this.server.close();
    process.stdout.write(`${this.name} ${this.chainId} stopped.\n`);
  }

  public async deployToken(
    name: string,
    symbol: string,
    wallet: ethers.Wallet
  ): Promise<MintableToken> {
    return MintableToken.createToken(name, symbol, wallet);
  }

  public async deploySignatureBridge(
    otherChain: LocalChain,
    localToken: MintableToken,
    otherToken: MintableToken,
    localWallet: ethers.Wallet,
    otherWallet: ethers.Wallet
  ): Promise<Bridges.SignatureBridge> {
    const gitRoot = child
      .execSync('git rev-parse --show-toplevel')
      .toString()
      .trim();
    localWallet.connect(this.provider());
    otherWallet.connect(otherChain.provider());
    const bridgeInput: BridgeInput = {
      anchorInputs: {
        asset: {
          [this.chainId]: [localToken.contract.address],
          [otherChain.chainId]: [otherToken.contract.address],
        },
        anchorSizes: [ethers.utils.parseEther('1')],
      },
      chainIDs: [this.chainId, otherChain.chainId],
    };
    const deployerConfig: DeployerConfig = {
      [this.chainId]: localWallet,
      [otherChain.chainId]: otherWallet,
    };
    const initialGovernors: GovernorConfig = {
      [this.chainId]: localWallet,
      [otherChain.chainId]: otherWallet,
    };
    const zkComponents = await fetchComponentsFromFilePaths(
      path.resolve(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/bridge/2/poseidon_bridge_2.wasm'
      ),
      path.resolve(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/bridge/2/witness_calculator.js'
      ),
      path.resolve(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/bridge/2/circuit_final.zkey'
      )
    );

    const val = await Bridges.SignatureBridge.deployFixedDepositBridge(
      bridgeInput,
      deployerConfig,
      initialGovernors,
      zkComponents
    );
    this.signatureBridge = val;
    return val;
  }

  public async exportConfig(
    signatureBridge?: Bridges.SignatureBridge
  ): Promise<FullChainInfo> {
    const bridge = signatureBridge ?? this.signatureBridge;
    if (!bridge) {
      throw new Error('Signature bridge not deployed yet');
    }
    const localAnchor = bridge.getAnchor(
      this.chainId,
      ethers.utils.parseEther('1')
    );
    const side = bridge.getBridgeSide(this.chainId);
    const wallet = side.governor;
    const otherChainIds = Array.from(bridge.bridgeSides.keys()).filter(
      (chainId) => chainId !== this.chainId
    );
    const otherAnchors = otherChainIds.map(
      (chainId) =>
        [chainId, bridge.getAnchor(chainId, ethers.utils.parseEther('1'))] as [
          number,
          Anchors.Anchor
        ]
    );

    const chainInfo: FullChainInfo = {
      enabled: true,
      httpEndpoint: this.endpoint,
      wsEndpoint: this.endpoint.replace('http', 'ws'),
      chainId: this.chainId,
      beneficiary: wallet.address,
      privateKey: wallet.privateKey,
      contracts: [
        // first the local Anchor
        {
          contract: 'Anchor',
          address: localAnchor.getAddress(),
          deployedAt: 1,
          size: 1, // Ethers
          withdrawFeePercentage: 0,
          eventsWatcher: { enabled: true, pollingInterval: 1000 },
          linkedAnchors: otherAnchors.map(([chainId, anchor]) => ({
            chain: chainId.toString(),
            address: anchor.getAddress(),
          })),
        },
        // Next is the signature bridge contract.
        // TODO: add signature bridge to the config when the relayer get it.
      ],
    };
    return chainInfo;
  }

  public async writeConfig(
    path: string,
    signatureBridge?: Bridges.SignatureBridge
  ): Promise<void> {
    const config = await this.exportConfig(signatureBridge);
    // don't mind my typescript typing here XD
    type ConvertedContract = Omit<
      ConvertToKebabCase<Contract>,
      'events-watcher'
    > & {
      'events-watcher': ConvertToKebabCase<EventsWatcher>;
    };
    type ConvertedConfig = Omit<
      ConvertToKebabCase<typeof config>,
      'contracts'
    > & {
      contracts: ConvertedContract[];
    };
    type FullConfigFile = {
      evm: {
        // chainId as the chain identifier
        [key: number]: ConvertedConfig;
      };
    };
    const convertedConfig: ConvertedConfig = {
      enabled: config.enabled,
      'http-endpoint': config.httpEndpoint,
      'ws-endpoint': config.wsEndpoint,
      'chain-id': config.chainId,
      beneficiary: config.beneficiary,
      'private-key': config.privateKey,
      contracts: config.contracts.map((contract) => ({
        contract: contract.contract,
        address: contract.address,
        'deployed-at': contract.deployedAt,
        size: contract.size,
        'withdraw-gaslimit': '0x5B8D80',
        'withdraw-fee-percentage': contract.withdrawFeePercentage,
        'events-watcher': {
          enabled: contract.eventsWatcher.enabled,
          'polling-interval': contract.eventsWatcher.pollingInterval,
        },
        'linked-anchors': contract.linkedAnchors,
      })),
    };
    const fullConfigFile: FullConfigFile = {
      evm: {
        [this.chainId]: convertedConfig,
      },
    };
    const configString = JSON.stringify(fullConfigFile, null, 2);
    fs.writeFileSync(path, configString);
  }
}

type CamelToKebabCase<S extends string> = S extends `${infer T}${infer U}`
  ? `${T extends Capitalize<T> ? '-' : ''}${Lowercase<T>}${CamelToKebabCase<U>}`
  : S;

type ConvertToKebabCase<T> = {
  [P in keyof T as Lowercase<CamelToKebabCase<string & P>>]: T[P];
};

export type FullChainInfo = ChainInfo & {
  httpEndpoint: string;
  wsEndpoint: string;
  privateKey: string;
};
