/*
 * Copyright 2022 Webb Technologies Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
import fs from 'fs';
import ganache from 'ganache';
import { ethers } from 'ethers';
import { Server } from 'ganache';
import { Bridges, Utility, VBridge } from '@webb-tools/protocol-solidity';
import {
  BridgeInput,
  DeployerConfig,
  GovernorConfig,
} from '@webb-tools/interfaces';
import { MintableToken, GovernedTokenWrapper } from '@webb-tools/tokens';
import { fetchComponentsFromFilePaths } from '@webb-tools/utils';
import path from 'path';
import child from 'child_process';
import {
  ChainInfo,
  Contract,
  EnabledContracts,
  EventsWatcher,
  ProposalSigningBackend,
} from './webbRelayer';
import { ConvertToKebabCase } from './tsHacks';

export type GanacheAccounts = {
  balance: string;
  secretKey: string;
};

export type ExportedConfigOptions = {
  signatureBridge?: Bridges.SignatureBridge;
  signatureVBridge?: VBridge.VBridge;
  proposalSigningBackend: ProposalSigningBackend;
};

// Default Events watcher for the contracts.
export const defaultEventsWatcherValue: EventsWatcher = {
  enabled: true,
  pollingInterval: 1000,
  printProgressInterval: 60_000,
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
    if (options.enableLogging) {
      process.stdout.write(
        `Ganache(${networkId}) Started on http://127.0.0.1:${port} ..\n`
      );
    }
  });

  return ganacheServer;
}

type LocalChainOpts = {
  name: string;
  port: number;
  chainId: number;
  populatedAccounts: GanacheAccounts[];
  enableLogging?: boolean;
  enabledContracts: EnabledContracts[];
};

export class LocalChain {
  public readonly endpoint: string;
  private readonly server: Server<'ethereum'>;
  private signatureBridge: Bridges.SignatureBridge | null = null;
  private signatureVBridge: VBridge.VBridge | null = null;
  constructor(private readonly opts: LocalChainOpts) {
    this.endpoint = `http://127.0.0.1:${opts.port}`;
    this.server = startGanacheServer(
      opts.port,
      opts.chainId,
      opts.populatedAccounts,
      {
        enableLogging: opts.enableLogging,
      }
    );
  }

  public get name(): string {
    return this.opts.name;
  }

  public get chainId(): number {
    return Utility.getChainIdType(this.opts.chainId);
  }

  public get underlyingChainId(): number {
    return this.opts.chainId;
  }

  public provider(): ethers.providers.WebSocketProvider {
    return new ethers.providers.WebSocketProvider(this.endpoint);
  }

  public async stop() {
    await this.server.close();
  }

  public async deployToken(
    name: string,
    symbol: string,
    wallet: ethers.Wallet
  ): Promise<MintableToken> {
    return MintableToken.createToken(name, symbol, wallet);
  }

  public async deploySignatureVBridge(
    otherChain: LocalChain,
    localToken: MintableToken,
    otherToken: MintableToken,
    localWallet: ethers.Wallet,
    otherWallet: ethers.Wallet
  ): Promise<VBridge.VBridge> {
    const gitRoot = child
      .execSync('git rev-parse --show-toplevel')
      .toString()
      .trim();
    let webbTokens1 = new Map<number, GovernedTokenWrapper | undefined>();
    webbTokens1.set(this.chainId, null!);
    webbTokens1.set(otherChain.chainId, null!);
    const vBridgeInput: VBridge.VBridgeInput = {
      vAnchorInputs: {
        asset: {
          [this.chainId]: [localToken.contract.address],
          [otherChain.chainId]: [otherToken.contract.address],
        },
      },
      chainIDs: [this.chainId, otherChain.chainId],
      webbTokens: webbTokens1,
    };
    const deployerConfig: DeployerConfig = {
      [this.chainId]: localWallet,
      [otherChain.chainId]: otherWallet,
    };
    const initialGovernors: GovernorConfig = {
      [this.chainId]: localWallet,
      [otherChain.chainId]: otherWallet,
    };

    const witnessCalculatorPath_2 = path.join(
      gitRoot,
      'tests',
      'protocol-solidity-fixtures/fixtures/vanchor_2/2/witness_calculator.js'
    );

    const witnessCalculatorCjsPath_2 = path.join(
      gitRoot,
      'tests',
      'node_modules/@webb-tools/utils/witness_calculator_2.cjs'
    );
    // check if the cjs file exists, if not, copy the js file to the cjs file
    if (!fs.existsSync(witnessCalculatorCjsPath_2)) {
      fs.copyFileSync(witnessCalculatorPath_2, witnessCalculatorCjsPath_2);
    }

    const witnessCalculatorPath_16 = path.join(
      gitRoot,
      'tests',
      'protocol-solidity-fixtures/fixtures/vanchor_16/2/witness_calculator.js'
    );

    const witnessCalculatorCjsPath_16 = path.join(
      gitRoot,
      'tests',
      'node_modules/@webb-tools/utils/witness_calculator_16.cjs'
    );
    // check if the cjs file exists, if not, copy the js file to the cjs file
    if (!fs.existsSync(witnessCalculatorCjsPath_16)) {
      fs.copyFileSync(witnessCalculatorPath_16, witnessCalculatorCjsPath_16);
    }

    const zkComponents_2 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/vanchor_2/2/poseidon_vanchor_2_2.wasm'
      ),
      witnessCalculatorCjsPath_2,
      path.join(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/vanchor_2/2/circuit_final.zkey'
      )
    );

    const zkComponents_16 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/vanchor_16/2/poseidon_vanchor_16_2.wasm'
      ),
      witnessCalculatorCjsPath_16,
      path.join(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/vanchor_16/2/circuit_final.zkey'
      )
    );

    const vBridge = await VBridge.VBridge.deployVariableAnchorBridge(
      vBridgeInput,
      deployerConfig,
      initialGovernors,
      zkComponents_2,
      zkComponents_16
    );
    this.signatureVBridge = vBridge;
    return vBridge;
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
    // copy the witness_calculator.js file to @webb-tools/utils, but use the .cjs extension
    // to avoid the babel compiler to compile it.
    const witnessCalculatorPath = path.join(
      gitRoot,
      'tests',
      'protocol-solidity-fixtures/fixtures/anchor/2/witness_calculator.js'
    );
    const witnessCalculatorCjsPath = path.join(
      gitRoot,
      'tests',
      'node_modules/@webb-tools/utils/witness_calculator.cjs'
    );
    // check if the cjs file exists, if not, copy the js file to the cjs file
    if (!fs.existsSync(witnessCalculatorCjsPath)) {
      fs.copyFileSync(witnessCalculatorPath, witnessCalculatorCjsPath);
    }
    const zkComponents = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/anchor/2/poseidon_anchor_2.wasm'
      ),
      witnessCalculatorCjsPath,
      path.join(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/anchor/2/circuit_final.zkey'
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

  private async getAnchorChainConfig(
    opts: ExportedConfigOptions
  ): Promise<FullChainInfo> {
    const bridge = opts.signatureBridge ?? this.signatureBridge;
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

    const otherAnchors = otherChainIds.map((chainId) =>
      bridge.getAnchor(chainId, ethers.utils.parseEther('1'))
    );

    let contracts: Contract[] = [
      // first the local Anchor
      {
        contract: 'Anchor',
        address: localAnchor.getAddress(),
        deployedAt: 1,
        size: 1, // Ethers
        withdrawFeePercentage: 0,
        proposalSigningBackend: opts.proposalSigningBackend,
        eventsWatcher: defaultEventsWatcherValue,
        linkedAnchors: await Promise.all(
          otherAnchors.map(async (anchor) => ({
            chain: (await anchor.contract.getChainId()).toString(),
            address: anchor.getAddress(),
          }))
        ),
      },
      {
        contract: 'SignatureBridge',
        address: side.contract.address,
        deployedAt: 1,
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];

    const chainInfo: FullChainInfo = {
      enabled: true,
      httpEndpoint: this.endpoint,
      wsEndpoint: this.endpoint.replace('http', 'ws'),
      chainId: this.underlyingChainId,
      beneficiary: wallet.address,
      privateKey: wallet.privateKey,
      contracts: contracts,
    };
    return chainInfo;
  }

  private async getVAnchorChainConfig(
    opts: ExportedConfigOptions
  ): Promise<FullChainInfo> {
    const bridge = opts.signatureVBridge ?? this.signatureVBridge;
    if (!bridge) {
      throw new Error('Signature V bridge not deployed yet');
    }
    const localAnchor = bridge.getVAnchor(this.chainId);
    const side = bridge.getVBridgeSide(this.chainId);
    const wallet = side.governor;
    const otherChainIds = Array.from(bridge.vBridgeSides.keys()).filter(
      (chainId) => chainId !== this.chainId
    );
    const otherAnchors = otherChainIds.map((chainId) =>
      bridge.getVAnchor(chainId)
    );
    let contracts: Contract[] = [
      // first the local Anchor
      {
        contract: 'VAnchor',
        address: localAnchor.getAddress(),
        deployedAt: 1,
        size: 1, // Ethers
        withdrawFeePercentage: 0,
        proposalSigningBackend: opts.proposalSigningBackend,
        eventsWatcher: {
          enabled: true,
          pollingInterval: 1000,
          printProgressInterval: 60_000,
        },
        linkedAnchors: await Promise.all(
          otherAnchors.map(async (anchor) => ({
            chain: (await anchor.contract.getChainId()).toString(),
            address: anchor.getAddress(),
          }))
        ),
      },
      {
        contract: 'SignatureVBridge',
        address: side.contract.address,
        deployedAt: 1,
        eventsWatcher: {
          enabled: true,
          pollingInterval: 1000,
          printProgressInterval: 60_000,
        },
      },
    ];
    const chainInfo: FullChainInfo = {
      enabled: true,
      httpEndpoint: this.endpoint,
      wsEndpoint: this.endpoint.replace('http', 'ws'),
      chainId: this.underlyingChainId,
      beneficiary: wallet.address,
      privateKey: wallet.privateKey,
      contracts: contracts,
    };
    return chainInfo;
  }

  public async exportConfig(
    opts: ExportedConfigOptions
  ): Promise<FullChainInfo> {
    const chainInfo: FullChainInfo = {
      enabled: true,
      httpEndpoint: this.endpoint,
      wsEndpoint: this.endpoint.replace('http', 'ws'),
      chainId: this.underlyingChainId,
      beneficiary: '',
      privateKey: '',
      contracts: [],
    };
    for (const contract of this.opts.enabledContracts) {
      if (contract.contract === 'Anchor') {
        return this.getAnchorChainConfig(opts);
      }
      if (contract.contract == 'VAnchor') {
        return this.getVAnchorChainConfig(opts);
      }
    }
    return chainInfo;
  }

  public async writeConfig(
    path: string,
    opts: ExportedConfigOptions
  ): Promise<void> {
    const config = await this.exportConfig(opts);
    // don't mind my typescript typing here XD
    type ConvertedContract = Omit<
      ConvertToKebabCase<Contract>,
      'events-watcher' | 'proposal-signing-backend'
    > & {
      'events-watcher': ConvertToKebabCase<EventsWatcher>;
      'proposal-signing-backend'?: ConvertToKebabCase<ProposalSigningBackend>;
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
        'proposal-signing-backend':
          contract.proposalSigningBackend?.type === 'Mocked'
            ? {
                type: 'Mocked',
                'private-key': contract.proposalSigningBackend?.privateKey,
              }
            : contract.proposalSigningBackend?.type === 'DKGNode'
            ? {
                type: 'DKGNode',
                node: contract.proposalSigningBackend?.node,
              }
            : undefined,
        'withdraw-gaslimit': '0x5B8D80',
        'withdraw-fee-percentage': contract.withdrawFeePercentage,
        'events-watcher': {
          enabled: contract.eventsWatcher.enabled,
          'polling-interval': contract.eventsWatcher.pollingInterval,
          'print-progress-interval':
            contract.eventsWatcher.printProgressInterval,
        },
        'linked-anchors': contract.linkedAnchors,
      })),
    };
    const fullConfigFile: FullConfigFile = {
      evm: {
        [this.underlyingChainId]: convertedConfig,
      },
    };
    const configString = JSON.stringify(fullConfigFile, null, 2);
    fs.writeFileSync(path, configString);
  }
}

export type FullChainInfo = ChainInfo & {
  httpEndpoint: string;
  wsEndpoint: string;
  privateKey: string;
};
