/// A Helper Class to Start and Manage a Local Protocol Substrate Node.
/// This Could be through a Docker Container or a Local Compiled node.
import fs from 'fs';
import '@webb-tools/tangle-substrate-types';
import { spawn } from 'child_process';
import {
  EventsWatcher,
  LinkedAnchor,
  NodeInfo,
  Pallet,
  ProposalSigningBackend,
} from './webbRelayer.js';
import { LocalNodeOpts } from '@webb-tools/test-utils';
import { ECPairAPI, TinySecp256k1Interface, ECPairFactory } from 'ecpair';
import * as TinySecp256k1 from 'tiny-secp256k1';
import { ConvertToKebabCase } from './tsHacks.js';
import { SubstrateNodeBase } from './substrateNodeBase.js';

const TANGLE_DOCKER_IMAGE_URL =
  'ghcr.io/webb-tools/tangle/tangle-standalone-integration-tests:main';

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

export class LocalTangle extends SubstrateNodeBase<TypedEvent> {
  public static async start(opts: LocalNodeOpts): Promise<LocalTangle> {
    opts.ports = await super.makePorts(opts);
    // opts.usageMode.mode = 'docker'
    const startArgs: string[] = [];
    if (opts.usageMode.mode === 'docker') {
      LocalTangle.pullImage({
        forcePull: opts.usageMode.forcePullImage,
        image: TANGLE_DOCKER_IMAGE_URL,
      });
      const dockerArgs = [
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
        TANGLE_DOCKER_IMAGE_URL,
        'tangle-standalone',
        '--tmp',
        '--chain=relayer',
        '--rpc-cors',
        'all',
        '--ws-external',
        '--rpc-methods=unsafe',
        `--${opts.authority}`,
        ...startArgs,
      ];
      const proc = spawn('docker', dockerArgs);
      if (opts.enableLogging) {
        proc.stdout.on('data', (data: Buffer) => {
          console.log(data.toString());
        });
        proc.stderr.on('data', (data: Buffer) => {
          console.error(data.toString());
        });
      }
      return new LocalTangle(opts, proc);
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
      return new LocalTangle(opts, proc);
    }
  }

  public async fetchDkgPublicKey(): Promise<`0x${string}`> {
    const api = await super.api();
    const res = await api.query.dkg.dkgPublicKey();
    const json = res.toJSON() as [number, string];
    const tinysecp: TinySecp256k1Interface = TinySecp256k1;
    const ECPair: ECPairAPI = ECPairFactory(tinysecp);
    if (json && json[1] !== '0x') {
      const key = json[1];
      const dkgPubKey = ECPair.fromPublicKey(Buffer.from(key.slice(2), 'hex'), {
        compressed: false,
      }).publicKey.toString('hex');
      // now we remove the `04` prefix byte and return it.
      return `0x${dkgPubKey.slice(2)}`;
    } else {
      return `0x`;
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
      suri: config.suri,
      pallets: config.pallets.map((c: Pallet) => {
        const convertedPallet: ConvertedPallet = {
          pallet: c.pallet,
          'events-watcher': {
            enabled: c.eventsWatcher.enabled,
            'polling-interval': c.eventsWatcher.pollingInterval,
            'print-progress-interval': c.eventsWatcher.printProgressInterval,
            'sync-blocks-from': c.eventsWatcher.syncBlocksFrom,
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
                  'chain-id': c.proposalSigningBackend?.chainId,
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
      },
    };
    const configString = JSON.stringify(fullConfigFile, null, 2);
    fs.writeFileSync(path, configString);
  }
}

export type TypedEvent =
  | NewSession
  | NextPublicKeySubmitted
  | NextPublicKeySignatureSubmitted
  | PublicKeySubmitted
  | PublicKeyChanged
  | PublicKeySignatureChanged
  | ProposalSigned;

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

type ProposalSigned = {
  section: 'dkgProposalHandler';
  method: 'ProposalSigned';
};
