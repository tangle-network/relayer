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
import fetch from 'node-fetch';
import {
  type IVariableAnchorExtData,
  type IVariableAnchorPublicInputs as IEvmVariableAnchorPublicInputs,
} from '@webb-tools/interfaces';
import { ChildProcess, spawn } from 'child_process';
import { EventEmitter } from 'events';
import JSONStream from 'JSONStream';
import { BigNumber } from 'ethers';
import { ConvertToKebabCase } from './tsHacks';
import { padHexString } from '../lib/utils.js';

export type CommonConfig = {
  features?: FeaturesConfig;
  evmEtherscan?: EvmEtherscanConfig;
  port: number;
  assets?: { [key: string]: UnlistedAssetConfig };
};

/**
 * A required configuration object for relayer startup.
 * @param tmp - A flag to allow users to automatically cleanup synced blockchain state
 * @param configDir - The directory path for the relayer configuration
 * @param commonConfig - An object which represents the directory-wide options of a relayer config
 *                       to be written before startup. (e.g. 'features')
 * @param buildDir - optional switching between debug and release builds
 * @param showLogs - Enable logging
 * @param verbosity - Determine the level of logging
 */
export type WebbRelayerOptions = {
  tmp: boolean;
  configDir: string;
  commonConfig: CommonConfig;
  buildDir?: 'debug' | 'release';
  showLogs?: boolean;
  verbosity?: number;
};

export class WebbRelayer {
  readonly #process: ChildProcess;
  readonly #logs: RawEvent[] = [];
  readonly #eventEmitter = new EventEmitter();
  constructor(private readonly opts: WebbRelayerOptions) {
    // Write the folder-wide configuration for this relayer instance
    type WrittenCommonConfig = {
      features?: ConvertToKebabCase<FeaturesConfig>;
      'evm-etherscan'?: {
        [key: string]: ConvertToKebabCase<EtherscanApiConfig>;
      };
      port: number;
      assets?: {
        [key: string]: ConvertToKebabCase<UnlistedAssetConfig>;
      };
    };
    const commonConfigFile: WrittenCommonConfig = {
      features: {
        'data-query': opts.commonConfig.features?.dataQuery ?? true,
        'governance-relay': opts.commonConfig.features?.governanceRelay ?? true,
        'private-tx-relay': opts.commonConfig.features?.privateTxRelay ?? true,
      },
      'evm-etherscan': Object.fromEntries(
        Object.entries(opts.commonConfig.evmEtherscan ?? {}).map(
          ([key, { chainId, apiKey }]) => [
            key,
            { ...{ 'chain-id': chainId, 'api-key': apiKey } },
          ]
        )
      ),
      port: opts.commonConfig.port,
    };
    const configString = JSON.stringify(commonConfigFile, null, 2);
    fs.writeFileSync(path.join(opts.configDir, 'main.json'), configString);

    // Startup the relayer
    const verbosity = opts.verbosity ?? 3;
    const levels = ['error', 'warn', 'info', 'debug', 'trace'];
    const logLevel = levels[verbosity] ?? 'debug';
    this.#process = spawn(
      'cargo',
      [
        'run',
        '--bin',
        'webb-relayer',
        '--features',
        'integration-tests,cli,native-tls/vendored',
        '--',
        '-c',
        opts.configDir,
        opts.tmp ? '--tmp' : '',
        `-${'v'.repeat(verbosity)}`,
      ],
      {
        env: {
          RUST_LOG: `webb_probe=${logLevel}`,
          WEBB_PORT: `${opts.commonConfig.port}`,
          // allow us to override the env
          ...process.env,
        },
      }
    );
    if (this.opts.showLogs) {
      // log that we started
      process.stdout.write(
        `Webb relayer started on port ${opts.commonConfig.port}\n`
      );
    }
    this.#process.stdout
      ?.pipe(JSONStream.parse())
      .on('data', (parsedLog: UnparsedRawEvent) => {
        if (this.opts.showLogs) {
          process.stdout.write(`${JSON.stringify(parsedLog)}\n`);
        }
        if (parsedLog.target === 'webb_probe') {
          const rawEvent = {
            timestamp: new Date(parsedLog.timestamp),
            ...(parsedLog as any),
          } as RawEvent;
          this.#logs.push(rawEvent);
          this.#eventEmitter.emit(rawEvent.kind, rawEvent);
        }
      });
    this.#process.stderr?.on('data', (data) => {
      if (this.opts.showLogs) {
        process.stdout.write(`${data}\n`);
      }
    });

    this.#process.on('close', (code) => {
      if (this.opts.showLogs) {
        process.stdout.write(
          `Relayer ${opts.commonConfig.port} exited with code: ${code}\n`
        );
      }
    });
  }

  public async info(): Promise<WebbRelayerInfo> {
    const endpoint = `http://127.0.0.1:${this.opts.commonConfig.port}/api/v1/info`;
    const response = await fetch(endpoint);
    return response.json() as Promise<WebbRelayerInfo>;
  }
  // data querying api for evm
  public async getLeavesEvm(
    chainId: string,
    contractAddress: string,
    queryRange?: { start?: number; end?: number }
  ) {
    const endpoint = new URL(
      `http://127.0.0.1:${this.opts.commonConfig.port}/api/v1/leaves/evm/${chainId}/${contractAddress}`
    );
    if (queryRange) {
      if (queryRange.start) {
        endpoint.searchParams.append('start', queryRange.start.toString());
      }
      if (queryRange.end) {
        endpoint.searchParams.append('end', queryRange.end.toString());
      }
    }
    const response = await fetch(endpoint.toString());
    return response;
  }

  public async getEncryptedOutputsEvm(
    chainId: string,
    contractAddress: string,
    queryRange?: { start?: number; end?: number }
  ) {
    const endpoint = new URL(
      `http://127.0.0.1:${this.opts.commonConfig.port}/api/v1/encrypted_outputs/evm/${chainId}/${contractAddress}`
    );
    if (queryRange) {
      if (queryRange.start) {
        endpoint.searchParams.append('start', queryRange.start.toString());
      }
      if (queryRange.end) {
        endpoint.searchParams.append('end', queryRange.end.toString());
      }
    }
    const response = await fetch(endpoint.toString());
    return response;
  }

  public async getMetricsGathered() {
    const endpoint = `http://127.0.0.1:${this.opts.commonConfig.port}/api/v1/metrics`;
    const response = await fetch(endpoint);
    return response;
  }
  // API to fetch metrics for particular resource
  public async getResourceMetricsEvm(chainId: string, contractAddress: string) {
    const endpoint = `http://127.0.0.1:${this.opts.commonConfig.port}/api/v1/metrics/evm/${chainId}/${contractAddress}`;
    const response = await fetch(endpoint);
    return response;
  }

  public async getEvmFeeInfo(
    chainId: number,
    vanchor: string,
    gas_amount: BigNumber
  ) {
    const endpoint = `http://127.0.0.1:${this.opts.commonConfig.port}/api/v1/fee_info/evm/${chainId}/${vanchor}/${gas_amount}`;
    const response = await fetch(endpoint);
    return response;
  }

  // Post API to send private withdrawal tx to relayer for evm chain
  public async sendPrivateTxEvm(
    chainId: number,
    contractAddress: string,
    payload: WithdrawRequestPayloadEVM
  ) {
    const endpoint = `http://127.0.0.1:${this.opts.commonConfig.port}/api/v1/send/evm/${chainId}/${contractAddress}`;
    const response = fetch(endpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(payload),
    });
    return response;
  }

  // API to get transaction status for evm chain.
  public async getTxStatusEvm(chainId: number, transactionItemKey: string) {
    const endpoint = `http://127.0.0.1:${this.opts.commonConfig.port}/api/v1/tx/evm/${chainId}/${transactionItemKey}`;
    const response = await fetch(endpoint);
    return response;
  }

  public async stop(): Promise<void> {
    this.#process.kill('SIGINT');
  }

  public dumpLogs(): RawEvent[] {
    return this.#logs;
  }

  public clearLogs(): void {
    this.#logs.length = 0;
  }

  public async waitUntilReady(): Promise<void> {
    await this.waitForEvent({ kind: 'lifecycle', event: { started: true } });
  }

  public async waitForEvent(selector: EventSelector): Promise<RawEvent> {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    return new Promise((resolve, _reject) => {
      const listener = (rawEvent: RawEvent) => {
        const exactSameEvent = Object.keys(selector.event).every((key) => {
          const a = selector.event?.[key];
          const b = rawEvent[key];
          return a === b;
        });
        if (exactSameEvent) {
          // remove listener
          this.#eventEmitter.removeListener(selector.kind, listener);
          resolve(rawEvent);
        }
      };
      // add listener
      this.#eventEmitter.prependListener(selector.kind, listener);
      // first try to find the event in the logs
      const event = this.#logs.find((log) => {
        const isTheSameKind = log.kind === selector.kind;
        const isTheSameEvent = Object.keys(selector.event).every((key) => {
          return selector.event[key] === log[key];
        });

        return isTheSameKind && isTheSameEvent;
      });
      if (event) {
        // remove listener
        this.#eventEmitter.removeListener(selector.kind, listener);
        resolve(event);
      }
    });
  }

  public vanchorWithdrawPayload(
    publicInputs: IEvmVariableAnchorPublicInputs,
    extData: IVariableAnchorExtData
  ): WithdrawRequestPayloadEVM {
    return {
      vAnchor: {
        extData: {
          recipient: extData.recipient,
          relayer: extData.relayer,
          extAmount: extData.extAmount.replace('0x', ''),
          fee: extData.fee,
          refund: extData.refund,
          token: extData.token,
          encryptedOutput1: extData.encryptedOutput1,
          encryptedOutput2: extData.encryptedOutput2,
        },
        proofData: {
          proof: publicInputs.proof,
          extensionRoots: publicInputs.extensionRoots,
          extDataHash: padHexString(publicInputs.extDataHash.toHexString()),
          publicAmount: publicInputs.publicAmount,
          roots: publicInputs.roots,
          outputCommitments: publicInputs.outputCommitments.map((output) =>
            padHexString(output.toHexString())
          ),
          inputNullifiers: publicInputs.inputNullifiers.map((nullifier) =>
            padHexString(nullifier.toHexString())
          ),
        },
      },
    };
  }
}

export function calculateRelayerFees(
  denomination: string,
  feePercentage: number
): BigNumber {
  const principleBig = BigNumber.from(denomination);
  const withdrawFeeMill = feePercentage * 1000000;
  const withdrawFeeMillBig = BigNumber.from(withdrawFeeMill);
  const feeBigMill = principleBig.mul(withdrawFeeMillBig);
  const feeBig = feeBigMill.div(BigNumber.from(1000000));
  return feeBig;
}

interface UnparsedRawEvent {
  timestamp: string;
  target: string;
  kind: string;
  [key: string]: any;
}

export interface RawEvent {
  timestamp: Date;
  level: 'ERROR' | 'INFO' | 'WARN' | 'DEBUG' | 'TRACE';
  kind: EventKind;
  target: EventTarget;
  [key: string]: any;
}

type EventKind =
  | 'lifecycle'
  | 'sync'
  | 'relay_tx'
  | 'signing_backend'
  | 'tx_queue'
  | 'private_tx'
  | 'leaves_store'
  | 'signing_backend'
  | 'signature_bridge'
  | 'encrypted_outputs_store'
  | 'retry';

type EventTarget = 'webb_probe';

export type EventSelector = {
  kind: EventKind;
  event?: any;
};

export interface FeaturesConfig {
  dataQuery?: boolean;
  governanceRelay?: boolean;
  privateTxRelay?: boolean;
}

export interface SmartAnchorUpdatesConfig {
  enabled?: boolean;
  minTimeDelay?: number;
  maxTimeDelay?: number;
  initialTimeDelay?: number;
  timeDelayWindowSize?: number;
}

export interface EtherscanApiConfig {
  chainId: number;
  apiKey: string;
}

export interface UnlistedAssetConfig {
  name: string;
  decimals: number;
  price: number;
}

export interface EvmEtherscanConfig {
  [key: string]: EtherscanApiConfig;
}

export interface RelayerFeeConfig {
  relayerProfitPercent: number;
  maxRefundAmount: number;
}

export interface WebbRelayerInfo {
  evm: Evm;
  substrate: Substrate;
}

export interface LeavesCacheResponse {
  leaves: [`0x${string}`];
  lastQueriedBlock: string;
}

export interface EncryptedOutputsCacheResponse {
  encryptedOutputs: [string];
  lastQueriedBloc: string;
}

export interface RelayerMetricResponse {
  metrics: string;
}

export interface ResourceMetricResponse {
  totalGasSpent: string;
  totalFeeEarned: string;
  accountBalance: string;
}

export interface WithdrawTxSuccessResponse {
  kind: 'success';
  status: string;
  message: string;
  itemKey: string;
}

export interface WithdrawTxFailureResponse {
  kind: 'failure';
  status: string;
  message: string;
  reason: string;
}

export type WithdrawRequestPayloadEVM = {
  vAnchor: {
    extData: {
      recipient: string;
      relayer: string;
      extAmount: string;
      fee: string;
      refund: string;
      token: string;
      encryptedOutput1: string;
      encryptedOutput2: string;
    };
    proofData: {
      proof: string;
      extensionRoots: string;
      extDataHash: string;
      publicAmount: string;
      roots: string;
      outputCommitments: string[];
      inputNullifiers: string[];
    };
  };
};

export interface TransactionStatusResponse {
  status: TxStatus;
  itemKey: string;
}

type TxStatus =
  | { kind: 'pending' }
  | { kind: 'processing'; step: string; progress: number }
  | { kind: 'failed'; reason: string }
  | { kind: 'processed'; txHash: string };

export interface Evm {
  [key: string]: ChainInfo;
}

export interface ChainInfo {
  name: string;
  enabled: boolean;
  chainId: number;
  beneficiary?: string;
  contracts: Contract[];
  blockConfirmations: number;
}

export interface EvmFeeInfo {
  estimatedFee: string;
  gasPrice: string;
  refundExchangeRate: string;
  maxRefund: string;
  timestamp: string;
}

export interface Contract {
  contract: ContractKind;
  address: string;
  deployedAt: number;
  eventsWatcher: EventsWatcher;
  size?: number;
  proposalSigningBackend?: ProposalSigningBackend;
  linkedAnchors?: LinkedAnchor[];
  smartAnchorUpdates?: SmartAnchorUpdatesConfig;
}

export interface EventsWatcher {
  enabled: boolean;
  pollingInterval: number;
  printProgressInterval?: number;
  syncBlocksFrom?: number;
}

export type RawResourceId = {
  type: 'Raw';
  resourceId: string;
};

export type EvmLinkedAnchor = {
  type: 'Evm';
  chainId: string;
  address: string;
};

export type LinkedAnchor = RawResourceId | EvmLinkedAnchor;

export interface Substrate {
  [key: string]: NodeInfo;
}

export interface NodeInfo {
  enabled: boolean;
  chainId: number;
  beneficiary?: string;
  pallets: Pallet[];
}

export interface Pallet {
  pallet: PalletKind;
  eventsWatcher: EventsWatcher;
}

export interface EnabledContracts {
  contract: ContractKind;
}

// Default WithdrawlFeeConfig for the tx relaying.
export const defaultRelayerFeeConfigValue: RelayerFeeConfig = {
  maxRefundAmount: 0,
  relayerProfitPercent: 0,
};

type ContractKind =
  | 'Anchor'
  | 'SignatureBridge'
  | 'GovernanceBravoDelegate'
  | 'VAnchor';

type PalletKind = 'DKG' | 'DKGProposals' | 'DKGProposalHandler';

export type DKGProposalSigningBackend = {
  type: 'DKGNode';
  chainId: number;
}; /** DKG Node name in the config */

export type MockedProposalSigningBackend = {
  type: 'Mocked';
  privateKey: string;
}; /** Signer private key */

export type ProposalSigningBackend =
  | DKGProposalSigningBackend
  | MockedProposalSigningBackend;
