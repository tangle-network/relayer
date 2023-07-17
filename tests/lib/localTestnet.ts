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
import path from 'path';
import child from 'child_process';
import { BigNumberish, ethers, Wallet } from 'ethers';
import {
  IVariableAnchorExtData,
  IVariableAnchorPublicInputs,
} from '@webb-tools/interfaces';
import { VAnchor as VAnchorClass } from '@webb-tools/anchors';
import { type VAnchor } from '@webb-tools/contracts';
import { DeployerConfig, GovernorConfig } from '@webb-tools/interfaces';
import { FungibleTokenWrapper, MintableToken } from '@webb-tools/tokens';
import {
  fetchComponentsFromFilePaths,
  getChainIdType,
  Keypair,
  Utxo,
} from '@webb-tools/utils';
import {
  ChainInfo,
  Contract,
  EnabledContracts,
  EventsWatcher,
  LinkedAnchor,
  ProposalSigningBackend,
  SmartAnchorUpdatesConfig,
  WithdrawConfig,
} from './webbRelayer';
import { ConvertToKebabCase } from './tsHacks';
import { hexToU8a, u8aToHex } from '@polkadot/util';
import { TokenConfig, VBridge, VBridgeInput } from '@webb-tools/vbridge';
import { LocalEvmChain } from '@webb-tools/evm-test-utils';

export type GanacheAccounts = {
  balance: string;
  secretKey: string;
};

export type ExportedConfigOptions = {
  signatureVBridge?: VBridge<VAnchor>;
  proposalSigningBackend?: ProposalSigningBackend;
  withdrawConfig?: WithdrawConfig;
  relayerWallet?: Wallet;
  linkedAnchors?: LinkedAnchor[];
  blockConfirmations?: number;
  privateKey?: string;
  smartAnchorUpdates?: SmartAnchorUpdatesConfig;
  httpEndpoints?: string[];
  txQueueConfig?: TxQueueConfig;
};

// Default Events watcher for the contracts.
export const defaultEventsWatcherValue: EventsWatcher = {
  enabled: true,
  pollingInterval: 1000,
  printProgressInterval: 60_000,
};

// Default configuration for Evm Tx queue
export const defaultEvmTxQueueConfig: TxQueueConfig = {
  maxSleepInterval: 1500,
  pollingInterval: 15000,
};

// Tx queue configurations
export interface TxQueueConfig {
  maxSleepInterval: number;
  pollingInterval: number;
}

type LocalChainOpts = {
  name: string;
  port: number;
  chainId: number;
  populatedAccounts: GanacheAccounts[];
  enableLogging?: boolean;
  enabledContracts: EnabledContracts[];
  evmOptions?: any;
};

export class LocalChain {
  private localEvmChain: LocalEvmChain;
  public readonly endpoint: string;
  private signatureVBridge: VBridge<VAnchor> | null = null;
  private constructor(
    private readonly opts: LocalChainOpts,
    localEvmChain: LocalEvmChain
  ) {
    this.localEvmChain = localEvmChain;
    this.endpoint = `http://127.0.0.1:${opts.port}`;
  }

  public static async init(opts: LocalChainOpts) {
    const evmChain = await LocalEvmChain.init(
      opts.name,
      opts.chainId,
      opts.populatedAccounts,
      opts.evmOptions
    );
    const localChain = new LocalChain(opts, evmChain);
    return localChain;
  }

  public get name(): string {
    return this.opts.name;
  }

  public get chainId(): number {
    return getChainIdType(this.opts.chainId);
  }

  public get underlyingChainId(): number {
    return this.opts.chainId;
  }

  public provider(): ethers.providers.WebSocketProvider {
    return new ethers.providers.WebSocketProvider(this.endpoint, {
      name: this.opts.name,
      chainId: this.underlyingChainId,
    });
  }

  public async stop() {
    await this.localEvmChain.stop();
  }

  public async deployToken(name: string, symbol: string): Promise<TokenConfig> {
    return {
      name,
      symbol,
    };
  }

  public async deployVBridge(
    localToken: TokenConfig,
    unwrappedToken: MintableToken,
    localWallet: ethers.Wallet,
    initialGovernor: ethers.Wallet
  ): Promise<VBridge<VAnchor>> {
    const gitRoot = child
      .execSync('git rev-parse --show-toplevel')
      .toString()
      .trim();
    const tokenConfigs = new Map<number, TokenConfig | undefined>();
    tokenConfigs.set(this.chainId, localToken);
    const vBridgeInput: VBridgeInput = {
      vAnchorInputs: {
        asset: {
          [this.chainId]: [unwrappedToken.contract.address],
        },
      },
      chainIds: [this.chainId],
      webbTokens: new Map<number, FungibleTokenWrapper | undefined>(),
      tokenConfigs: tokenConfigs,
    };
    const deployerConfig: DeployerConfig = {
      [this.chainId]: localWallet,
    };
    const deployerGovernors: GovernorConfig = {
      [this.chainId]: initialGovernor.address,
    };

    const witnessCalculatorCjsPath_2 = path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_2/2/witness_calculator.cjs'
    );

    const witnessCalculatorCjsPath_16 = path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_16/2/witness_calculator.cjs'
    );

    const zkComponents_2 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_2/2/poseidon_vanchor_2_2.wasm'
      ),
      witnessCalculatorCjsPath_2,
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_2/2/circuit_final.zkey'
      )
    );

    const zkComponents_16 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_16/2/poseidon_vanchor_16_2.wasm'
      ),
      witnessCalculatorCjsPath_16,
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_16/2/circuit_final.zkey'
      )
    );

    const vBridge = await VBridge.deployVariableAnchorBridge<VAnchor>(
      vBridgeInput,
      deployerConfig,
      deployerGovernors,
      zkComponents_2,
      zkComponents_16
    );

    return vBridge;
  }

  public async deploySignatureVBridge(
    otherChain: LocalChain,
    wrappedToken1: TokenConfig,
    wrappedToken2: TokenConfig,
    localWallet: ethers.Wallet,
    otherWallet: ethers.Wallet,
    unwrappedToken1: MintableToken,
    unwrappedToken2: MintableToken,
    initialGovernors?: GovernorConfig
  ): Promise<VBridge<VAnchor>> {
    const gitRoot = child
      .execSync('git rev-parse --show-toplevel')
      .toString()
      .trim();
    const tokenConfigs = new Map<number, TokenConfig | undefined>();
    tokenConfigs.set(this.chainId, wrappedToken1);
    tokenConfigs.set(otherChain.chainId, wrappedToken2);
    const vBridgeInput: VBridgeInput = {
      vAnchorInputs: {
        asset: {
          [this.chainId]: [unwrappedToken1.contract.address],
          [otherChain.chainId]: [unwrappedToken2.contract.address],
        },
      },
      chainIds: [this.chainId, otherChain.chainId],
      tokenConfigs: tokenConfigs,
      webbTokens: new Map<number, FungibleTokenWrapper | undefined>(),
    };
    const deployerConfig: DeployerConfig = {
      [this.chainId]: localWallet,
      [otherChain.chainId]: otherWallet,
    };
    const deployerGovernors: GovernorConfig = {
      [this.chainId]: localWallet.address,
      [otherChain.chainId]: otherWallet.address,
    };

    const witnessCalculatorCjsPath_2 = path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_2/2/witness_calculator.cjs'
    );

    const witnessCalculatorCjsPath_16 = path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_16/2/witness_calculator.cjs'
    );

    const zkComponents_2 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_2/2/poseidon_vanchor_2_2.wasm'
      ),
      witnessCalculatorCjsPath_2,
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_2/2/circuit_final.zkey'
      )
    );

    const zkComponents_16 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_16/2/poseidon_vanchor_16_2.wasm'
      ),
      witnessCalculatorCjsPath_16,
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_16/2/circuit_final.zkey'
      )
    );

    const vBridge = await VBridge.deployVariableAnchorBridge<VAnchor>(
      vBridgeInput,
      deployerConfig,
      deployerGovernors,
      zkComponents_2,
      zkComponents_16
    );

    this.signatureVBridge = vBridge;
    if (initialGovernors) {
      const govEntries = Object.entries(initialGovernors);

      for (const entry of govEntries) {
        const chainBridgeSide = this.signatureVBridge.getVBridgeSide(
          Number(entry[0])
        );
        const nonce = await chainBridgeSide.contract.proposalNonce();
        const initialGovernor = entry[1];
        const governorAddress =
          typeof initialGovernor === 'string'
            ? initialGovernor
            : initialGovernor!.address!;
        const governorNonce =
          typeof initialGovernor === 'string'
            ? nonce.toNumber()
            : initialGovernor!.nonce!;
        // eslint-disable-next-line no-constant-condition
        while (true) {
          try {
            const tx = await chainBridgeSide.transferOwnership(
              governorAddress,
              governorNonce
            );
            await tx.wait();
            break;
          } catch (e) {
            console.log(e);
          }
        }
      }
    }

    return vBridge;
  }

  public static async deployManySignatureVBridge(
    chains: LocalChain[],
    wrappedTokens: TokenConfig[],
    unwrappedTokens: string[],
    deployerWallets: ethers.Wallet[],
    initialGovernors: GovernorConfig
  ): Promise<VBridge<VAnchor>> {
    const gitRoot = child
      .execSync('git rev-parse --show-toplevel')
      .toString()
      .trim();
    const tokenConfigs = new Map<number, TokenConfig | undefined>();
    for (let i = 0; i < chains.length; i++) {
      tokenConfigs.set(chains[i]!.chainId, wrappedTokens[i]);
    }
    const asset: VBridgeInput['vAnchorInputs']['asset'] = {};
    for (let i = 0; i < chains.length; i++) {
      asset[chains[i]!.chainId] = [unwrappedTokens[i]!];
    }
    const vBridgeInput: VBridgeInput = {
      vAnchorInputs: {
        asset,
      },
      chainIds: chains.map((chain) => chain.chainId),
      tokenConfigs: tokenConfigs,
      webbTokens: new Map<number, FungibleTokenWrapper | undefined>(),
    };
    const deployerConfig: DeployerConfig = {};
    for (let i = 0; i < chains.length; i++) {
      deployerConfig[chains[i]!.chainId] = deployerWallets[i]!;
    }

    const witnessCalculatorCjsPath_2 = path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_2/8/witness_calculator.cjs'
    );

    const witnessCalculatorCjsPath_16 = path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_16/8/witness_calculator.cjs'
    );

    const zkComponents_2 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_2/8/poseidon_vanchor_2_8.wasm'
      ),
      witnessCalculatorCjsPath_2,
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_2/8/circuit_final.zkey'
      )
    );

    const zkComponents_16 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_16/8/poseidon_vanchor_16_8.wasm'
      ),
      witnessCalculatorCjsPath_16,
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_16/8/circuit_final.zkey'
      )
    );

    const vBridge = await VBridge.deployVariableAnchorBridge<VAnchor>(
      vBridgeInput,
      deployerConfig,
      initialGovernors,
      zkComponents_2,
      zkComponents_16
    );

    return vBridge;
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
    const wallet = opts.relayerWallet ?? side.governor;

    const contracts: Contract[] = [
      // first the local Anchor
      {
        contract: 'VAnchor',
        address: localAnchor.getAddress(),
        deployedAt: 1,
        size: 1, // Ethers
        proposalSigningBackend: opts.proposalSigningBackend,
        withdrawConfig: opts.withdrawConfig,
        eventsWatcher: {
          enabled: true,
          pollingInterval: 1000,
          printProgressInterval: 7000,
        },
        linkedAnchors: opts.linkedAnchors,
        smartAnchorUpdates: opts.smartAnchorUpdates,
      },
      {
        contract: 'SignatureBridge',
        address: side.contract.address,
        deployedAt: 1,
        eventsWatcher: {
          enabled: true,
          pollingInterval: 1000,
          printProgressInterval: 7000,
        },
      },
    ];
    const chainInfo: FullChainInfo = {
      name: this.underlyingChainId.toString(),
      enabled: true,
      httpEndpoint: opts.httpEndpoints ?? [this.endpoint],
      wsEndpoint: this.endpoint.replace('http', 'ws'),
      blockConfirmations: opts.blockConfirmations ?? 0,
      chainId: this.underlyingChainId,
      beneficiary: (wallet as ethers.Wallet).address,
      privateKey: (wallet as ethers.Wallet).privateKey,
      contracts: contracts,
      txQueue: opts.txQueueConfig ?? defaultEvmTxQueueConfig,
    };
    return chainInfo;
  }

  public async exportConfig(
    opts: ExportedConfigOptions
  ): Promise<FullChainInfo> {
    const chainInfo: FullChainInfo = {
      name: this.underlyingChainId.toString(),
      enabled: true,
      httpEndpoint: opts.httpEndpoints ?? [this.endpoint],
      wsEndpoint: this.endpoint.replace('http', 'ws'),
      blockConfirmations: opts.blockConfirmations ?? 1,
      chainId: this.underlyingChainId,
      beneficiary: '',
      privateKey: opts.privateKey ?? '',
      contracts: [],
      txQueue: defaultEvmTxQueueConfig,
    };
    for (const contract of this.opts.enabledContracts) {
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
    type ConvertedLinkedAnchor = ConvertToKebabCase<LinkedAnchor>;
    type ConvertedContract = Omit<
      ConvertToKebabCase<Contract>,
      | 'events-watcher'
      | 'proposal-signing-backend'
      | 'withdraw-config'
      | 'linked-anchors'
      | 'smart-anchor-updates'
    > & {
      'events-watcher': ConvertToKebabCase<EventsWatcher>;
      'proposal-signing-backend'?: ConvertToKebabCase<ProposalSigningBackend>;
      'withdraw-config'?: ConvertToKebabCase<WithdrawConfig>;
      'linked-anchors'?: ConvertedLinkedAnchor[];
      'smart-anchor-updates'?: ConvertToKebabCase<SmartAnchorUpdatesConfig>;
    };
    type ConvertedTxQueueConfig = ConvertToKebabCase<TxQueueConfig>;
    type ConvertedConfig = Omit<
      ConvertToKebabCase<typeof config>,
      'contracts' | 'tx-queue'
    > & {
      contracts: ConvertedContract[];
      'tx-queue': ConvertedTxQueueConfig;
    };
    type FullConfigFile = {
      evm: {
        // chainId as the chain identifier
        [key: number]: ConvertedConfig;
      };
    };

    const convertedConfig: ConvertedConfig = {
      name: config.name,
      enabled: config.enabled,
      'http-endpoint': config.httpEndpoint,
      'ws-endpoint': config.wsEndpoint,
      'chain-id': config.chainId,
      'block-confirmations': config.blockConfirmations,
      beneficiary: config.beneficiary,
      'private-key': config.privateKey,
      'tx-queue': {
        'max-sleep-interval': config.txQueue.maxSleepInterval,
        'polling-interval': config.txQueue.pollingInterval,
      },
      contracts: config.contracts.map(
        (contract): ConvertedContract => ({
          contract: contract.contract,
          address: contract.address,
          'deployed-at': contract.deployedAt,
          'proposal-signing-backend':
            contract.proposalSigningBackend?.type === 'Mocked'
              ? {
                  type: 'Mocked',
                  'private-key': contract.proposalSigningBackend?.privateKey,
                }
              : contract.proposalSigningBackend?.type === 'DKGNode'
              ? {
                  type: 'DKGNode',
                  'chain-id': contract.proposalSigningBackend?.chainId,
                }
              : undefined,
          'events-watcher': {
            enabled: contract.eventsWatcher.enabled,
            'polling-interval': contract.eventsWatcher.pollingInterval,
            'print-progress-interval':
              contract.eventsWatcher.printProgressInterval,
          },
          'smart-anchor-updates': {
            enabled: contract.smartAnchorUpdates?.enabled ?? false,
            'initial-time-delay': contract.smartAnchorUpdates?.initialTimeDelay,
            'max-time-delay': contract.smartAnchorUpdates?.maxTimeDelay,
            'min-time-delay': contract.smartAnchorUpdates?.minTimeDelay,
            'time-delay-window-size':
              contract.smartAnchorUpdates?.timeDelayWindowSize,
          },
          'linked-anchors': contract?.linkedAnchors?.map(
            (anchor: LinkedAnchor) =>
              anchor.type === 'Evm'
                ? {
                    'chain-id': anchor.chainId,
                    type: 'Evm',
                    address: anchor.address,
                  }
                : {
                    type: 'Raw',
                    'resource-id': anchor.resourceId,
                  }
          ),
        })
      ),
    };
    const fullConfigFile: FullConfigFile = {
      evm: {
        [this.underlyingChainId]: convertedConfig,
      },
    };
    const configString = JSON.stringify(fullConfigFile, null, 2);
    fs.writeFileSync(path, configString);
  }

  public async getVAnchor(signer: ethers.Wallet): Promise<VAnchorClass> {
    if (!this.signatureVBridge) {
      throw new Error('Signature VBridge not initialized');
    }
    const gitRoot = child
      .execSync('git rev-parse --show-toplevel')
      .toString()
      .trim();
    const witnessCalculatorCjsPath_2 = path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_2/2/witness_calculator.cjs'
    );

    const witnessCalculatorCjsPath_16 = path.join(
      gitRoot,
      'tests',
      'solidity-fixtures/vanchor_16/2/witness_calculator.cjs'
    );

    const zkComponents_2 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_2/2/poseidon_vanchor_2_2.wasm'
      ),
      witnessCalculatorCjsPath_2,
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_2/2/circuit_final.zkey'
      )
    );

    const zkComponents_16 = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_16/2/poseidon_vanchor_16_2.wasm'
      ),
      witnessCalculatorCjsPath_16,
      path.join(
        gitRoot,
        'tests',
        'solidity-fixtures/vanchor_16/2/circuit_final.zkey'
      )
    );

    const vanchorContract = this.signatureVBridge.getVAnchor(this.chainId);
    return VAnchorClass.connect(
      vanchorContract.getAddress(),
      zkComponents_2,
      zkComponents_16,
      signer
    );
  }
}

export type FullChainInfo = ChainInfo & {
  httpEndpoint: string[];
  wsEndpoint: string;
  privateKey: string;
  blockConfirmations: number;
  txQueue: TxQueueConfig;
};

export async function setupVanchorEvmTx(
  depositUtxo: Utxo,
  srcChain: LocalChain,
  destChain: LocalChain,
  randomKeypair: Keypair,
  srcVanchor: VAnchorClass,
  destVanchor: VAnchorClass,
  relayerWallet2: Wallet,
  tokenAddress: string,
  fee: BigNumberish,
  refund: BigNumberish,
  recipient: string
): Promise<{
  extData: IVariableAnchorExtData;
  publicInputs: IVariableAnchorPublicInputs;
}> {
  const dummyOutput1 = Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Circom',
    amount: '0',
    chainId: destChain.chainId.toString(),
    keypair: randomKeypair,
  });

  const dummyOutput2 = Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Circom',
    amount: '0',
    chainId: destChain.chainId.toString(),
    keypair: randomKeypair,
  });

  const dummyInput = Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Circom',
    amount: '0',
    chainId: destChain.chainId.toString(),
    originChainId: destChain.chainId.toString(),
    keypair: randomKeypair,
  });

  // Populate the leavesMap for generating the zkp against the source chain
  //
  const leaves1 = srcVanchor.tree
    .elements()
    .map((el) => hexToU8a(el.toHexString()));

  const leaves2 = destVanchor.tree
    .elements()
    .map((el) => hexToU8a(el.toHexString()));

  const depositUtxoIndex = srcVanchor.tree.getIndexByElement(
    u8aToHex(depositUtxo.commitment)
  );

  const regeneratedUtxo = Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Circom',
    amount: depositUtxo.amount,
    chainId: depositUtxo.chainId,
    originChainId: depositUtxo.originChainId,
    blinding: hexToU8a(depositUtxo.blinding),
    keypair: randomKeypair,
    index: depositUtxoIndex.toString(),
  });

  const leavesMap = {
    [srcChain.chainId]: leaves1,
    [destChain.chainId]: leaves2,
  };

  const { extData, publicInputs } = await destVanchor.setupTransaction(
    [regeneratedUtxo, dummyInput],
    [dummyOutput1, dummyOutput2],
    fee,
    refund,
    recipient,
    relayerWallet2.address,
    tokenAddress,
    leavesMap
  );

  return { extData, publicInputs };
}
