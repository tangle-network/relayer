/// A Helper Class to Start and Manage a Local Protocol Substrate Node.
/// This Could be through a Docker Container or a Local Compiled node.
import fs from 'fs';

import { spawn } from 'child_process';
import { EventsWatcher, LinkedAnchor, NodeInfo, Pallet, ProposalSigningBackend } from './webbRelayer.js';
import { 
  LocalProtocolSubstrate as BaseLocalSubstrate,
  LocalNodeOpts,
} from '@webb-tools/test-utils';

import { ConvertToKebabCase } from './tsHacks.js';

const STANDALONE_DOCKER_IMAGE_URL =
  'ghcr.io/webb-tools/protocol-substrate-standalone-node:edge';

type ExportedConfigOptions = {
  suri: string;
  proposalSigningBackend?: ProposalSigningBackend;
  linkedAnchors?: LinkedAnchor[];
  chainId: number;
  enabledPallets?: Pallet[];
};

type FullNodeInfo = NodeInfo & {
  name: string;
  httpEndpoint: string;
  wsEndpoint: string;
  suri: string;
  chainId: number;
};

export class LocalProtocolSubstrate extends BaseLocalSubstrate {
  public static async start(
    opts: LocalNodeOpts
  ): Promise<LocalProtocolSubstrate> {
    opts.ports = await super.makePorts(opts);
    const startArgs: string[] = [];
    if (opts.usageMode.mode === 'docker') {
      LocalProtocolSubstrate.pullImage({
        forcePull: opts.usageMode.forcePullImage,
        image: STANDALONE_DOCKER_IMAGE_URL,
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
        '--rpc-methods=unsafe',
        `--${opts.authority}`
      );
      if (!opts.isManual) {
        const proc = spawn('docker', startArgs, {});
        if (opts.enableLogging) {
          proc.stdout.on('data', (data: Buffer) => {
            console.log(data.toString());
          });
          proc.stderr.on('data', (data: Buffer) => {
            console.error(data.toString());
          });
        }
        return new LocalProtocolSubstrate(opts, proc);
      }

      return new LocalProtocolSubstrate(opts);
    } else {
      startArgs.push(
        '--tmp',
        '--rpc-cors',
        'all',
        '--rpc-methods=unsafe',
        '--ws-external',
        `--ws-port=${opts.ports.ws}`,
        `--rpc-port=${opts.ports.http}`,
        `--port=${opts.ports.p2p}`,
        `--${opts.authority}`
      );
      const proc = spawn(opts.usageMode.nodePath, startArgs);
      if (opts.enableLogging) {
        proc.stdout.on('data', (data: Buffer) => {
          console.log(data.toString());
        });
        proc.stderr.on('data', (data: Buffer) => {
          console.error(data.toString());
        });
      }
      return new LocalProtocolSubstrate(opts, proc);
    }
  }
  // get chainId
  public async getChainId(): Promise<number> {
    const api = await super.api();
    const chainId = api.consts.linkableTreeBn254.chainIdentifier.toNumber();
    return chainId;
  }

  public async exportConfig(
    opts: ExportedConfigOptions
  ): Promise<FullNodeInfo> {
    const ports = this.opts.ports as { ws: number; http: number; p2p: number };
    const enabledPallets: Pallet[] = [];
    for (const p of opts.enabledPallets ?? []) {
      if (p.pallet != 'SignatureBridge') {
        (p.linkedAnchors = opts.linkedAnchors),
          (p.proposalSigningBackend = opts.proposalSigningBackend);
        enabledPallets.push(p);
      } else {
        enabledPallets.push(p);
      }
    }
    const nodeInfo: FullNodeInfo = {
      name: 'localSubstrate',
      enabled: true,
      httpEndpoint: `http://127.0.0.1:${ports.http}`,
      wsEndpoint: `ws://127.0.0.1:${ports.ws}`,
      runtime: 'WebbProtocol',
      pallets: enabledPallets,
      suri: opts.suri,
      chainId: opts.chainId,
    };
    return nodeInfo;
  }

  public async writeConfig(
    path: string,
    opts: ExportedConfigOptions
  ): Promise<void> {
    const config = await this.exportConfig(opts);
    // don't mind my typescript typing here XD
    type ConvertedLinkedAnchor = ConvertToKebabCase<LinkedAnchor>;
    type ConvertedPallet = Omit<
      ConvertToKebabCase<Pallet>,
      'events-watcher' | 'proposal-signing-backend' | 'linked-anchors'
    > & {
      'events-watcher': ConvertToKebabCase<EventsWatcher>;
      'proposal-signing-backend'?: ConvertToKebabCase<ProposalSigningBackend>;
      'linked-anchors'?: ConvertedLinkedAnchor[];
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
      name: config.name,
      enabled: config.enabled,
      'http-endpoint': config.httpEndpoint,
      'ws-endpoint': config.wsEndpoint,
      'chain-id': config.chainId,
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

          'linked-anchors': c.linkedAnchors?.map((anchor: LinkedAnchor) =>
            anchor.type === 'Evm'
              ? {
                  'chain-id': anchor.chainId,
                  type: 'Evm',
                  address: anchor.address,
                }
              : anchor.type === 'Substrate'
              ? {
                  type: 'Substrate',
                  'chain-id': anchor.chainId,
                  'tree-id': anchor.treeId,
                  pallet: anchor.pallet,
                }
              : {
                  type: 'Raw',
                  'resource-id': anchor.resourceId,
                }
          ),
        };
        return convertedPallet;
      }),
    };
    const fullConfigFile: FullConfigFile = {
      substrate: {
        [this.opts.name]: convertedConfig,
      }
    };
    const configString = JSON.stringify(fullConfigFile, null, 2);
    fs.writeFileSync(path, configString);
  }
}

export type TypedEvent = MixerBn254DepositEvent | MixerBn254WithdrawEvent;

type MixerBn254DepositEvent = { section: 'mixerBn254'; method: 'Deposit' };
type MixerBn254WithdrawEvent = { section: 'mixerBn254'; method: 'Withdraw' };
