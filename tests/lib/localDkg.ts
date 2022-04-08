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
/// A Helper Class to Start and Manage a Local DKG Node.
/// This Could be through a Docker Container or a Local Compiled node.

import getPort, { portNumbers } from 'get-port';
import { ChildProcess, execSync, spawn } from 'child_process';
import { ApiPromise, WsProvider } from '@polkadot/api';

const DKG_STANDALONE_DOCKER_IMAGE_URL =
  'ghcr.io/webb-tools/dkg-standalone-node:edge';

export type DockerMode = {
  mode: 'docker';
  forcePullImage: boolean;
};

export type HostMode = {
  mode: 'host';
  nodePath: string;
};

export type LocalDkgOptions = {
  ports:
    | {
        ws: number;
        http: number;
        p2p: number;
      }
    | 'auto';
  authority: 'alice' | 'bob' | 'charlie';
  usageMode: DockerMode | HostMode;
};

export class LocalDkg {
  #api: ApiPromise | null = null;
  private constructor(
    private readonly opts: LocalDkgOptions,
    private readonly process: ChildProcess
  ) {}

  public static async start(opts: LocalDkgOptions): Promise<LocalDkg> {
    if (opts.ports === 'auto') {
      opts.ports = {
        ws: await getPort({ port: portNumbers(9944, 9999) }),
        http: await getPort({ port: portNumbers(9933, 9999) }),
        p2p: await getPort({ port: portNumbers(30333, 30399) }),
      };
    }
    const startArgs: string[] = [
      '-ldkg=debug',
      '-ldkg_metadata=debug',
      '-lruntime::offchain=debug',
      '-ldkg_proposal_handler=debug',
    ];
    if (opts.usageMode.mode === 'docker') {
      LocalDkg.pullDkgImage({ frocePull: opts.usageMode.forcePullImage });
      startArgs.push(
        'run',
        '--rm',
        '--name',
        `${opts.authority}-node`,
        '-p',
        `${opts.ports.ws}:9944`,
        '-p',
        `${opts.ports.http}:9933`,
        '-p',
        `${opts.ports.p2p}:30333`,
        DKG_STANDALONE_DOCKER_IMAGE_URL,
        'dkg-standalone-node',
        '--tmp',
        '--rpc-cors',
        'all',
        '--ws-external',
        `--${opts.authority}`
      );
      const proc = spawn('docker', startArgs);
      return new LocalDkg(opts, proc);
    } else {
      startArgs.push(
        '--tmp',
        '--rpc-cors',
        'all',
        '--ws-external',
        `--ws-port=${opts.ports.ws}`,
        `--rpc-port=${opts.ports.http}`,
        `--port=${opts.ports.p2p}`,
        `--${opts.authority}`
      );
      const proc = spawn(opts.usageMode.nodePath, startArgs);
      return new LocalDkg(opts, proc);
    }
  }

  public async api(): Promise<ApiPromise> {
    if (this.#api) {
      return this.#api;
    }
    const wsPort = (this.opts.ports as { ws: number }).ws;
    this.#api = await ApiPromise.create({
      provider: new WsProvider(`ws://localhost:${wsPort}`),
    });
    await this.#api.isReady;
    return this.#api;
  }

  public async stop(): Promise<void> {
    await this.#api?.disconnect();
    this.#api = null;
    this.process.kill('SIGINT');
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

  private static checkIfDkgImageExists(): boolean {
    const result = execSync('docker images', { encoding: 'utf8' });
    return result.includes(DKG_STANDALONE_DOCKER_IMAGE_URL);
  }

  private static pullDkgImage(
    opts: { frocePull: boolean } = { frocePull: false }
  ): void {
    if (!this.checkIfDkgImageExists() || opts.frocePull) {
      execSync(`docker pull ${DKG_STANDALONE_DOCKER_IMAGE_URL}`, {
        encoding: 'utf8',
      });
    }
  }
}

export type TypedEvent =
  | NewSession
  | NextPublicKeySubmitted
  | NextPublicKeySignatureSubmitted
  | PublicKeySubmitted
  | PublicKeyChanged
  | PublicKeySignatureChanged;

type NewSession = { section: 'session'; method: 'NewSession' };
type NextPublicKeySubmitted = {
  section: 'dkg';
  method: 'NextPublicKeySubmitted';
};
type NextPublicKeySignatureSubmitted = {
  section: 'dkg';
  method: 'NextPublicKeySignatureSubmitted';
};
type PublicKeySubmitted = { section: 'dkg'; method: 'PublicKeySubmitted' };
type PublicKeyChanged = { section: 'dkg'; method: 'PublicKeyChanged' };
type PublicKeySignatureChanged = {
  section: 'dkg';
  method: 'PublicKeySignatureChanged';
};
