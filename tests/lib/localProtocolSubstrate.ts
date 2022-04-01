/// A Helper Class to Start and Manage a Local Protocol Substrate Node.
/// This Could be through a Docker Container or a Local Compiled node.

import fs from 'fs';
import getPort, { portNumbers } from 'get-port';
import { ChildProcess, execSync, spawn } from 'child_process';
import { ApiPromise, WsProvider } from '@polkadot/api';
import { EventsWatcher, NodeInfo, Pallet } from './webbRelayer';
import { ConvertToKebabCase } from './tsHacks';

const STANDALONE_DOCKER_IMAGE_URL =
  'ghcr.io/webb-tools/protocol-substrate-standalone-node:edge';

export type DockerMode = {
  mode: 'docker';
  forcePullImage: boolean;
};

export type HostMode = {
  mode: 'host';
  nodePath: string;
};

export type NodeOptions = {
  name: string;
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

export class LocalProtocolSubstrate {
  #api: ApiPromise | null = null;
  private constructor(
    private readonly opts: NodeOptions,
    private readonly process: ChildProcess
  ) {}

  public get name(): string {
    return this.opts.name;
  }

  public static async start(
    opts: NodeOptions
  ): Promise<LocalProtocolSubstrate> {
    if (opts.ports === 'auto') {
      opts.ports = {
        ws: await getPort({ port: portNumbers(9944, 9999) }),
        http: await getPort({ port: portNumbers(9933, 9999) }),
        p2p: await getPort({ port: portNumbers(30333, 30399) }),
      };
    }
    const startArgs: string[] = [];
    if (opts.usageMode.mode === 'docker') {
      LocalProtocolSubstrate.pullDkgImage({
        frocePull: opts.usageMode.forcePullImage,
      });
      startArgs.push(
        'run',
        '--rm',
        '--name',
        `${opts.authority}-node-${opts.ports.ws}`,
        '-p',
        `${opts.ports.ws}:9944`,
        '-p',
        `${opts.ports.http}:9933`,
        '-p',
        `${opts.ports.p2p}:30333`,
        STANDALONE_DOCKER_IMAGE_URL,
        'webb-standalone-node',
        '--tmp',
        '--rpc-cors',
        'all',
        '--ws-external',
        `--${opts.authority}`
      );
      const proc = spawn('docker', startArgs, {});
      return new LocalProtocolSubstrate(opts, proc);
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
      return new LocalProtocolSubstrate(opts, proc);
    }
  }

  public async api(): Promise<ApiPromise> {
    if (this.#api) {
      return this.#api;
    }
    const ports = this.opts.ports as { ws: number; http: number; p2p: number };
    this.#api = await ApiPromise.create({
      provider: new WsProvider(`ws://127.0.0.1:${ports.ws}`),
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
      },
    });
    // await this.#api.isReady;
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

  public async exportConfig(suri: string): Promise<FullNodeInfo> {
    const ports = this.opts.ports as { ws: number; http: number; p2p: number };
    const nodeInfo: FullNodeInfo = {
      enabled: true,
      httpEndpoint: `http://127.0.0.1:${ports.http}`,
      wsEndpoint: `ws://127.0.0.1:${ports.ws}`,
      runtime: 'WebbProtocol',
      pallets: [],
      suri,
    };
    return nodeInfo;
  }

  public async writeConfig({
    path,
    suri,
  }: {
    path: string;
    suri: string;
  }): Promise<void> {
    const config = await this.exportConfig(suri);
    // don't mind my typescript typing here XD
    type ConvertedPallet = Omit<
      ConvertToKebabCase<Pallet>,
      'events-watcher'
    > & {
      'events-watcher': ConvertToKebabCase<EventsWatcher>;
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
      pallets: config.pallets.map((pallet: Pallet) => {
        const convertedPallet: ConvertedPallet = {
          pallet: pallet.pallet,
          'events-watcher': {
            enabled: pallet.eventsWatcher.enabled,
            'polling-interval': pallet.eventsWatcher.pollingInterval,
          },
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

  private static checkIfDkgImageExists(): boolean {
    const result = execSync('docker images', { encoding: 'utf8' });
    return result.includes(STANDALONE_DOCKER_IMAGE_URL);
  }

  private static pullDkgImage(
    opts: { frocePull: boolean } = { frocePull: false }
  ): void {
    if (!this.checkIfDkgImageExists() || opts.frocePull) {
      execSync(`docker pull ${STANDALONE_DOCKER_IMAGE_URL}`, {
        encoding: 'utf8',
      });
    }
  }
}

export type FullNodeInfo = NodeInfo & {
  httpEndpoint: string;
  wsEndpoint: string;
  suri: string;
};

export type TypedEvent = MixerBn254DepositEvent | MixerBn254WithdrawEvent;

type MixerBn254DepositEvent = { section: 'mixerBn254'; method: 'Deposit' };
type MixerBn254WithdrawEvent = { section: 'mixerBn254'; method: 'Withdraw' };
