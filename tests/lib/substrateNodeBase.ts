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
import getPort, { portNumbers } from 'get-port';
import { ChildProcess, execSync } from 'child_process';
import { ApiPromise, Keyring, WsProvider } from '@polkadot/api';
import {
  EventsWatcher,
  NodeInfo,
  Pallet,
  ProposalSigningBackend,
  SubstrateLinkedAnchor,
} from './webbRelayer.js';
import { ConvertToKebabCase } from './tsHacks.js';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { options } from '@webb-tools/api';

export type DockerMode = {
  mode: 'docker';
  forcePullImage: boolean;
};

export type HostMode = {
  mode: 'host';
  nodePath: string;
};

export type UsageMode = DockerMode | HostMode;

export type LocalNodeOpts = {
  name: string;
  ports:
    | {
        ws: number;
        http: number;
        p2p: number;
      }
    | 'auto';
  authority: 'alice' | 'bob' | 'charlie';
  usageMode: UsageMode;
  enableLogging?: boolean;
  isManual?: boolean; // for manual connection to the substrate node using 9944
  enabledPallets?: Pallet[];
};

export type SubstrateEvent = {
  section: string;
  method: string;
};

export type ExportedConfigOptions = {
  suri: string;
  proposalSigningBackend?: ProposalSigningBackend;
  linkedAnchors?: SubstrateLinkedAnchor[];
};

// Default Events watcher for the pallets.
export const defaultEventsWatcherValue: EventsWatcher = {
  enabled: true,
  pollingInterval: 3000,
};

export abstract class SubstrateNodeBase<TypedEvent extends SubstrateEvent> {
  #api: ApiPromise | null = null;
  constructor(
    protected readonly opts: LocalNodeOpts,
    private readonly proc?: ChildProcess
  ) {}

  public get name(): string {
    return this.opts.name;
  }

  public static async makePorts(
    opts: LocalNodeOpts
  ): Promise<{ ws: number; http: number; p2p: number }> {
    return opts.ports === 'auto'
      ? {
          ws: await getPort({ port: portNumbers(9944, 9999) }),
          http: await getPort({ port: portNumbers(9933, 9999) }),
          p2p: await getPort({ port: portNumbers(30333, 30399) }),
        }
      : (opts.ports as { ws: number; http: number; p2p: number });
  }

  public async api(): Promise<ApiPromise> {
    const ports = this.opts.ports as { ws: number; http: number; p2p: number };
    const host = '127.0.0.1';
    if (this.opts.isManual) {
      return await createApiPromise(`ws://${host}:${ports.ws}`);
    }

    if (this.#api) {
      return this.#api;
    }
    this.#api = await createApiPromise(`ws://${host}:${ports.ws}`);
    return this.#api;
  }

  public async stop(): Promise<void> {
    await this.#api?.disconnect();
    this.#api = null;
    if (this.proc) this.proc.kill('SIGINT');
  }

  public async waitForEvent(typedEvent: TypedEvent): Promise<void> {
    const api = await this.api();
    return new Promise(async (resolve, _) => {
      // Subscribe to system events via storage
      const unsub: any = await api.query.system!.events!((events: any[]) => {
        // Loop through the Vec<EventRecord>
        events.forEach((record: any) => {
          const { event } = record;
          if (
            event.section === typedEvent.section &&
            event.method === typedEvent.method
          ) {
            // Unsubscribe from the storage
            unsub();
            // Resolve the promise
            resolve();
          }
        });
      });
    });
  }

  public async executeTransaction(
    tx: SubmittableExtrinsic<'promise'>
  ): Promise<string> {
    const api = await this.api();
    return new Promise((resolve, reject) => {
      tx.send(({ status, dispatchError }) => {
        // status would still be set, but in the case of error we can shortcut
        // to just check it (so an error would indicate InBlock or Finalized)
        if (dispatchError) {
          if (dispatchError.isModule) {
            // for module errors, we have the section indexed, lookup
            const decoded = api.registry.findMetaError(dispatchError.asModule);
            const { docs, name, section } = decoded;
            reject(`${section}.${name}: ${docs.join(' ')}`);
          } else {
            // Other, CannotLookup, BadOrigin, no extra info
            reject(dispatchError.toString());
          }
        }
        if (status.isFinalized && !dispatchError) {
          resolve(status.asFinalized.toString());
        }
      });
    });
  }

  public async sudoExecuteTransaction(
    tx: SubmittableExtrinsic<'promise'>
  ): Promise<string> {
    const api = await this.api();
    const keyring = new Keyring({ type: 'sr25519' });
    const sudoKey = keyring.addFromUri(`//Alice`);
    const sudoCall = api.tx.sudo!.sudo!(tx);
    return new Promise((resolve, reject) => {
      sudoCall.signAndSend(
        sudoKey,
        { nonce: -1 },
        ({ status, dispatchError }) => {
          // status would still be set, but in the case of error we can shortcut
          // to just check it (so an error would indicate InBlock or Finalized)
          if (dispatchError) {
            if (dispatchError.isModule) {
              // for module errors, we have the section indexed, lookup
              const decoded = api.registry.findMetaError(
                dispatchError.asModule
              );
              const { docs, name, section } = decoded;
              reject(`${section}.${name}: ${docs.join(' ')}`);
            } else {
              // Other, CannotLookup, BadOrigin, no extra info
              reject(dispatchError.toString());
            }
          }
          if (status.isFinalized && !dispatchError) {
            resolve(status.asFinalized.toString());
          }
        }
      );
    });
  }

  abstract exportConfig(opts: ExportedConfigOptions): Promise<FullNodeInfo>;

  public async writeConfig(
    path: string,
    opts: ExportedConfigOptions
  ): Promise<void> {
    const config = await this.exportConfig(opts);
    // don't mind my typescript typing here XD
    type ConvertedPallet = Omit<
      ConvertToKebabCase<Pallet>,
      'events-watcher' | 'proposal-signing-backend'
    > & {
      'events-watcher': ConvertToKebabCase<EventsWatcher>;
      'proposal-signing-backend'?: ConvertToKebabCase<ProposalSigningBackend>;
    };
    type ConvertedConfig = Omit<
      ConvertToKebabCase<typeof config>,
      'pallets'
    > & {
      pallets: ConvertedPallet[];
    };
    type FullConfigFile = {
      substrate: {
        [key: string]: ConvertedConfig;
      };
    };
    const convertedConfig: ConvertedConfig = {
      enabled: config.enabled,
      'http-endpoint': config.httpEndpoint,
      'ws-endpoint': config.wsEndpoint,
      runtime: config.runtime,
      suri: config.suri,
      pallets: config.pallets.map((c: Pallet) => {
        const convertedPallet: ConvertedPallet = {
          pallet: c.pallet,
          'events-watcher': {
            enabled: c.eventsWatcher.enabled,
            'polling-interval': c.eventsWatcher.pollingInterval,
          },
          'proposal-signing-backend':
            c.proposalSigningBackend?.type === 'Mocked'
              ? {
                  type: 'Mocked',
                  'private-key': c.proposalSigningBackend?.privateKey,
                }
              : c.proposalSigningBackend?.type === 'DKGNode'
              ? {
                  type: 'DKGNode',
                  node: c.proposalSigningBackend?.node,
                }
              : undefined,

          'linked-anchors': c.linkedAnchors,
        };
        return convertedPallet;
      }),
    };
    const fullConfigFile: FullConfigFile = {
      substrate: {
        [this.opts.name]: convertedConfig,
      },
    };
    const configString = JSON.stringify(fullConfigFile, null, 2);
    fs.writeFileSync(path, configString);
  }

  protected static checkIfDkgImageExists(image: string): boolean {
    const result = execSync('docker images', { encoding: 'utf8' });
    return result.includes(image);
  }

  protected static pullDkgImage(opts: {
    frocePull: boolean;
    image: string;
  }): void {
    if (!this.checkIfDkgImageExists(opts.image) || opts.frocePull) {
      execSync(`docker pull ${opts.image}`, {
        encoding: 'utf8',
      });
    }
  }
}

async function createApiPromise(endpoint: string) {
  return ApiPromise.create(
    // @ts-ignore
    options({
      provider: new WsProvider(endpoint) as any,
      rpc: {
        mt: {
          getLeaves: {
            description: 'Query for the tree leaves',
            params: [
              {
                name: 'tree_id',
                type: 'u32',
                isOptional: false,
              },
              {
                name: 'from',
                type: 'u32',
                isOptional: false,
              },
              {
                name: 'to',
                type: 'u32',
                isOptional: false,
              },
              {
                name: 'at',
                type: 'Hash',
                isOptional: true,
              },
            ],
            type: 'Vec<[u8; 32]>',
          },
        },
        lt: {
          getNeighborRoots: {
            description: 'Query for the neighbor roots',
            params: [
              {
                name: 'tree_id',
                type: 'u32',
                isOptional: false,
              },
              {
                name: 'at',
                type: 'Hash',
                isOptional: true,
              },
            ],
            type: 'Vec<[u8; 32]>',
          },
        },
      },
    })
  );
}

export type FullNodeInfo = NodeInfo & {
  httpEndpoint: string;
  wsEndpoint: string;
  suri: string;
};
