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

import { ChildProcess, execSync } from 'child_process';

import { ApiPromise, Keyring, WsProvider } from '@polkadot/api';

import { SubmittableExtrinsic } from '@polkadot/api/types';
import '@webb-tools/tangle-substrate-types';
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
    rpc: number;
    p2p: number;
  }
  | 'auto';
  authority: 'alice' | 'bob' | 'charlie';
  usageMode: UsageMode;
  enableLogging?: boolean;
  isManual?: boolean; // for manual connection to the substrate node using 9944
};

export type SubstrateEvent = {
  section: string;
  method: string;
};

export abstract class SubstrateNodeBase<TypedEvent extends SubstrateEvent> {
  #api: ApiPromise | null = null;
  constructor(
    protected readonly opts: LocalNodeOpts,
    protected readonly proc?: ChildProcess
  ) { }

  public get name(): string {
    return this.opts.name;
  }

  public static async makePorts(
    opts: LocalNodeOpts
  ): Promise<LocalNodeOpts['ports']> {
    // Dynamic import used for commonjs compatibility
    const getPort = await import('get-port');
    const { portNumbers } = getPort;

    return opts.ports === 'auto'
      ? {
        p2p: await getPort.default({ port: portNumbers(30333, 30399) }),
        rpc: await getPort.default({ port: portNumbers(9944, 9999) }),
      }
      : (opts.ports as LocalNodeOpts['ports']);
  }

  public async api(): Promise<ApiPromise> {
    const ports = this.opts.ports as { rpc: number };
    const host = '127.0.0.1';

    if (this.opts.isManual) {
      return await createApiPromise(`ws://${host}:${ports.rpc}`);
    }

    if (this.#api) {
      return this.#api;
    }

    this.#api = await createApiPromise(`ws://${host}:${ports.rpc}`);

    return this.#api;
  }

  public async stop(): Promise<void> {
    await this.#api?.disconnect();
    this.#api = null;

    if (this.proc) {
      this.proc.kill('SIGINT');
    }
  }

  public async waitForEvent(typedEvent: TypedEvent): Promise<void> {
    const api = await this.api();

    return new Promise<void>((resolve) => {
      const unsub = api.query.system.events((events) => {
        events.forEach((record) => {
          const { event } = record;
          if (
            event.section === typedEvent.section &&
            event.method === typedEvent.method
          ) {
            unsub.then(() => resolve());
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
      tx.send(({ dispatchError, status }) => {
        // status would still be set, but in the case of error we can shortcut
        // to just check it (so an error would indicate InBlock or Finalized)
        if (dispatchError) {
          if (dispatchError.isModule) {
            // for module errors, we have the section indexed, lookup
            const decoded = api.registry.findMetaError(dispatchError.asModule);
            const { docs, name, section } = decoded;

            reject(new Error(`${section}.${name}: ${docs.join(' ')}`));
          } else {
            // Other, CannotLookup, BadOrigin, no extra info
            reject(dispatchError.toString());
          }
        }

        if (status.isFinalized && !dispatchError) {
          resolve(status.asFinalized.toString());
        }
      }).catch((e) => reject(e));
    });
  }

  public async sudoExecuteTransaction(
    tx: SubmittableExtrinsic<'promise'>
  ): Promise<string> {
    const api = await this.api();
    const keyring = new Keyring({ type: 'sr25519' });
    const sudoKey = keyring.addFromUri('//Alice');
    const sudoCall = api.tx.sudo.sudo(tx);
    return new Promise((resolve, reject) => {
      const unsub = sudoCall
        .signAndSend(sudoKey, { nonce: -1 }, ({ dispatchError, status }) => {
          // status would still be set, but in the case of error we can shortcut
          // to just check it (so an error would indicate InBlock or Finalized)
          if (dispatchError) {
            if (dispatchError.isModule) {
              // for module errors, we have the section indexed, lookup
              const decoded = api.registry.findMetaError(
                dispatchError.asModule
              );
              const { docs, name, section } = decoded;

              unsub.then(() =>
                reject(new Error(`${section}.${name}: ${docs.join(' ')}`))
              );
            } else {
              // Other, CannotLookup, BadOrigin, no extra info
              unsub.then(() => reject(dispatchError.toString()));
            }
          }

          if (status.isFinalized && !dispatchError) {
            unsub.then(() => resolve(status.asFinalized.toString()));
          }
        })
        .catch((e) => reject(e));
    });
  }

  protected static checkIfImageExists(image: string): boolean {
    const result = execSync('docker images', { encoding: 'utf8' });

    return result.includes(image);
  }

  protected static pullImage(opts: {
    forcePull: boolean;
    image: string;
  }): void {
    if (!this.checkIfImageExists(opts.image) || opts.forcePull) {
      execSync(`docker pull ${opts.image}`, {
        encoding: 'utf8',
      });
    }
  }
}

export async function createApiPromise(endpoint: string) {
  return ApiPromise.create({
    provider: new WsProvider(endpoint) as any,
  });
}
