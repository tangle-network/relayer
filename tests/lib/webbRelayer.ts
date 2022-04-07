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
import WebSocket from 'ws';
import fetch from 'node-fetch';
import {
  IFixedAnchorPublicInputs,
  IFixedAnchorExtData,
} from '@webb-tools/interfaces';
import { ChildProcess, spawn, execSync } from 'child_process';
import { EventEmitter } from 'events';
import JSONStream from 'JSONStream';
import { BigNumber } from 'ethers';

export type WebbRelayerOptions = {
  port: number;
  tmp: boolean;
  configDir: string;
  buildDir?: 'debug' | 'release';
  showLogs?: boolean;
  verbosity?: number;
};

export class WebbRelayer {
  readonly #process: ChildProcess;
  readonly #logs: RawEvent[] = [];
  readonly #eventEmitter = new EventEmitter();
  constructor(private readonly opts: WebbRelayerOptions) {
    const gitRoot = execSync('git rev-parse --show-toplevel').toString().trim();
    const buildDir = opts.buildDir ?? 'debug';
    const verbosity = opts.verbosity ?? 3;
    const levels = ['error', 'warn', 'info', 'debug', 'trace'];
    const logLevel = levels[verbosity] ?? 'debug';
    const relayerPath = `${gitRoot}/target/${buildDir}/webb-relayer`;
    this.#process = spawn(
      relayerPath,
      [
        '-c',
        opts.configDir,
        opts.tmp ? '--tmp' : '',
        `-${'v'.repeat(verbosity)}`,
      ],
      {
        env: {
          ...process.env,
          WEBB_PORT: `${this.opts.port}`,
          RUST_LOG: `webb_probe=${logLevel}`,
        },
      }
    );
    // log that we started
    process.stdout.write(`Webb relayer started on port ${this.opts.port}\n`);
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

    this.#process.on('close', (code) => {
      process.stdout.write(
        `Relayer ${this.opts.port} exited with code: ${code}\n`
      );
    });
  }

  public async info(): Promise<WebbRelayerInfo> {
    const endpoint = `http://127.0.0.1:${this.opts.port}/api/v1/info`;
    const response = await fetch(endpoint);
    return response.json() as Promise<WebbRelayerInfo>;
  }

  public async stop(): Promise<void> {
    this.#process.kill('SIGINT');
  }

  public dumpLogs(): RawEvent[] {
    return this.#logs;
  }

  public async waitUntilReady(): Promise<void> {
    await this.waitForEvent({ kind: 'lifecycle', event: { started: true } });
  }

  public async waitForEvent(selector: EventSelector): Promise<RawEvent> {
    return new Promise((resolve, _) => {
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

  public async ping(): Promise<void> {
    const wsEndpoint = `ws://127.0.0.1:${this.opts.port}/ws`;
    const ws = new WebSocket(wsEndpoint);
    await new Promise((resolve) => ws.once('open', resolve));
    return new Promise(async (resolve, reject) => {
      ws.on('error', reject);
      ws.on('message', (data) => {
        const o = JSON.parse(data.toString());
        const msg = parseRelayTxMessage(o);
        if (msg.kind === 'pong') {
          resolve();
        } else {
          reject(new Error(`Unexpected message: ${msg.kind}: ${data}`));
        }
      });
      ws.send(JSON.stringify({ ping: [] }));
    });
  }

  public async anchorWithdraw(
    chainName: string,
    anchorAddress: string,
    proof: `0x${string}`,
    publicInputs: IFixedAnchorPublicInputs,
    extData: IFixedAnchorExtData
  ): Promise<`0x${string}`> {
    const wsEndpoint = `ws://127.0.0.1:${this.opts.port}/ws`;
    // create a new websocket connection to the relayer.
    const ws = new WebSocket(wsEndpoint);
    await new Promise((resolve) => ws.once('open', resolve));
    const input = { chainName, anchorAddress, proof, publicInputs, extData };
    return txHashOrReject(ws, input);
  }

  public async substrateMixerWithdraw(inputs: {
    chain: string;
    id: number;
    proof: number[];
    root: number[];
    nullifierHash: number[];
    recipient: string;
    relayer: string;
    fee: number;
    refund: number;
  }): Promise<`0x${string}`> {
    const wsEndpoint = `ws://127.0.0.1:${this.opts.port}/ws`;
    // create a new websocket connection to the relayer.
    const ws = new WebSocket(wsEndpoint);
    await new Promise((resolve) => ws.once('open', resolve));
    return substrateTxHashOrReject(ws, inputs);
  }
}

export function calcualteRelayerFees(
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

async function txHashOrReject(
  ws: WebSocket,
  {
    chainName,
    anchorAddress,
    proof,
    publicInputs,
    extData,
  }: {
    chainName: string;
    anchorAddress: string;
    proof: `0x${string}`;
    publicInputs: IFixedAnchorPublicInputs;
    extData: IFixedAnchorExtData;
  }
): Promise<`0x${string}`> {
  return new Promise((resolve, reject) => {
    ws.on('error', reject);
    ws.on('message', (data) => {
      const o = JSON.parse(data.toString());
      const msg = parseRelayTxMessage(o);
      if (msg.kind === 'error') {
        ws.close();
        reject(msg.message);
      } else if (msg.kind === 'pong') {
        ws.close();
        // unreachable.
        reject('unreachable');
      } else if (msg.kind === 'network') {
        const networkError =
          msg.network === 'unsupportedChain' ||
          msg.network === 'unsupportedContract' ||
          msg.network === 'disconnected' ||
          msg.network === 'invalidRelayerAddress';
        const maybeFailed = msg.network as { failed: { reason: string } };
        if (networkError) {
          ws.close();
          reject(msg.network);
        } else if (maybeFailed.failed) {
          ws.close();
          reject(maybeFailed.failed.reason);
        }
      } else if (msg.kind === 'unimplemented') {
        ws.close();
        reject(msg.message);
      } else if (msg.kind === 'unknown') {
        ws.close();
        console.log(o);
        reject('Got unknown response from the relayer!');
      } else if (msg.kind === 'withdraw') {
        const isError =
          msg.withdraw === 'invalidMerkleRoots' ||
          msg.withdraw === 'droppedFromMemPool' ||
          (msg.withdraw as { errored: any }).errored;
        const success = msg.withdraw as {
          finalized: { txHash: `0x${string}` };
        };
        if (isError) {
          ws.close();
          reject(msg.withdraw);
        } else if (success.finalized) {
          ws.close();
          resolve(success.finalized.txHash);
        }
      }
    });
    const cmd = {
      evm: {
        anchorRelayTx: {
          chain: chainName,
          contract: anchorAddress,
          proof,
          roots: publicInputs._roots,
          nullifierHash: publicInputs._nullifierHash,
          extDataHash: publicInputs._extDataHash,
          refreshCommitment: extData._refreshCommitment,
          recipient: extData._recipient,
          relayer: extData._relayer,
          fee: extData._fee,
          refund: extData._refund,
        },
      },
    };
    ws.send(JSON.stringify(cmd));
  });
}

async function substrateTxHashOrReject(
  ws: WebSocket,
  input: any
): Promise<`0x${string}`> {
  return new Promise((resolve, reject) => {
    ws.on('error', reject);
    ws.on('message', (data) => {
      const o = JSON.parse(data.toString());
      const msg = parseRelayTxMessage(o);
      if (msg.kind === 'error') {
        ws.close();
        reject(msg.message);
      } else if (msg.kind === 'pong') {
        ws.close();
        // unreachable.
        reject('unreachable');
      } else if (msg.kind === 'network') {
        const networkError =
          msg.network === 'unsupportedChain' ||
          msg.network === 'unsupportedContract' ||
          msg.network === 'disconnected' ||
          msg.network === 'invalidRelayerAddress';
        const maybeFailed = msg.network as { failed: { reason: string } };
        if (networkError) {
          ws.close();
          reject(msg.network);
        } else if (maybeFailed.failed) {
          ws.close();
          reject(maybeFailed.failed.reason);
        }
      } else if (msg.kind === 'unimplemented') {
        ws.close();
        reject(msg.message);
      } else if (msg.kind === 'unknown') {
        ws.close();
        console.log(o);
        reject('Got unknown response from the relayer!');
      } else if (msg.kind === 'withdraw') {
        const isError =
          msg.withdraw === 'invalidMerkleRoots' ||
          msg.withdraw === 'droppedFromMemPool' ||
          (msg.withdraw as { errored: any }).errored;
        const success = msg.withdraw as {
          finalized: { txHash: `0x${string}` };
        };
        if (isError) {
          ws.close();
          reject(msg.withdraw);
        } else if (success.finalized) {
          ws.close();
          resolve(success.finalized.txHash);
        }
      }
    });
    const cmd = {
      substrate: {
        mixerRelayTx: {
          chain: input.chain,
          id: input.id,
          proof: input.proof,
          root: input.root,
          nullifierHash: input.nullifierHash,
          recipient: input.recipient,
          relayer: input.relayer,
          fee: input.fee,
          refund: input.refund,
        },
      },
    };
    ws.send(JSON.stringify(cmd));
  });
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
  | 'signature_bridge'
  | 'tx_queue';
type EventTarget = 'webb_probe';

export type EventSelector = {
  kind: EventKind;
  event?: any;
};

export interface WebbRelayerInfo {
  evm: Evm;
  substrate: Substrate;
}

export interface Evm {
  [key: string]: ChainInfo;
}

export interface ChainInfo {
  enabled: boolean;
  chainId: number;
  beneficiary?: string;
  contracts: Contract[];
}

export interface Contract {
  contract: ContractKind;
  address: string;
  deployedAt: number;
  eventsWatcher: EventsWatcher;
  size?: number;
  withdrawGaslimit?: `0x${string}`;
  withdrawFeePercentage?: number;
  'dkg-node'?: any;
  linkedAnchors?: LinkedAnchor[];
}

export interface EventsWatcher {
  enabled: boolean;
  pollingInterval: number;
}

export interface LinkedAnchor {
  chain: string;
  address: string;
}

export interface Substrate {
  [key: string]: NodeInfo;
}

export interface NodeInfo {
  enabled: boolean;
  runtime: RuntimeKind;
  pallets: Pallet[];
}

export interface Pallet {
  pallet: PalletKind;
  eventsWatcher: EventsWatcher;
}

type ContractKind =
  | 'Tornado'
  | 'Anchor'
  | 'SignatureBridge'
  | 'GovernanceBravoDelegate';
type RuntimeKind = 'DKG' | 'WebbProtocol';
type PalletKind = 'DKGProposals' | 'DKGProposalHandler';

type PongMessage = {
  kind: 'pong';
};

type NetworkMessage = {
  kind: 'network';
} & {
  network:
    | 'connecting'
    | 'connected'
    | { failed: { reason: string } }
    | 'disconnected'
    | 'unsupportedContract'
    | 'unsupportedChain'
    | 'invalidRelayerAddress';
};

type WithdrawMessage = {
  kind: 'withdraw';
} & {
  withdraw:
    | 'sent'
    | { submitted: { txHash: string } }
    | { finalized: { txHash: string } }
    | 'valid'
    | 'invalidMerkleRoots'
    | 'droppedFromMemPool'
    | { errored: { code: number; reason: string } };
};

type ErrorMessage = {
  kind: 'error';
} & { message: string };

type UnimplementedMessage = {
  kind: 'unimplemented';
} & { message: string };

type ParsedRelayerMessage =
  | PongMessage
  | NetworkMessage
  | WithdrawMessage
  | ErrorMessage
  | UnimplementedMessage
  | { kind: 'unknown' };

export type DKGSigningBackend = string; /** DKG Node name in the config */
export type MockedSigningBackend = { signer: string }; /** Signer private key */
export type SigningBackend = DKGSigningBackend | MockedSigningBackend;
function parseRelayTxMessage(o: any): ParsedRelayerMessage {
  if (o.pong) {
    return { kind: 'pong' };
  } else if (o.network) {
    return {
      kind: 'network',
      network: o.network,
    };
  } else if (o.withdraw) {
    return {
      kind: 'withdraw',
      withdraw: o.withdraw,
    };
  } else if (o.error) {
    return {
      kind: 'error',
      message: o.error,
    };
  } else if (o.unimplemented) {
    return {
      kind: 'unimplemented',
      message: o.unimplemented,
    };
  } else {
    return { kind: 'unknown' };
  }
}
