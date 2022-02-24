import { ChildProcess, spawn, execSync } from 'child_process';
import { EventEmitter } from 'events';
import JSONStream from 'JSONStream';

export type WebbRelayerOptions = {
  port: number;
  tmp: boolean;
  configDir: string;
  buildDir?: 'debug' | 'release';
};
export class WebbRelayer {
  readonly #process: ChildProcess;
  readonly #logs: RawEvent[] = [];
  readonly #eventEmitter = new EventEmitter();
  constructor(private readonly opts: WebbRelayerOptions) {
    const gitRoot = execSync('git rev-parse --show-toplevel').toString().trim();
    const buildDir = opts.buildDir ?? 'debug';
    const relayerPath = `${gitRoot}/target/${buildDir}/webb-relayer`;
    this.#process = spawn(
      relayerPath,
      ['-c', opts.configDir, opts.tmp ? '--tmp' : '', '-vvv'],
      {
        env: {
          ...process.env,
          WEBB_PORT: `${this.opts.port}`,
          RUST_LOG: `webb_probe=trace`,
        },
        killSignal: 'SIGILL',
      }
    );
    // log that we started
    process.stdout.write(`Webb relayer started on port ${this.opts.port}\n`);
    this.#process.stdout
      ?.pipe(JSONStream.parse())
      .on('data', (parsedLog: UnparsedRawEvent) => {
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
    return response.json();
  }

  public async stop(): Promise<void> {
    this.#process.kill('SIGINT');
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

type EventKind = 'lifecycle' | 'sync';
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
  size: number;
  withdrawFeePercentage?: number;
  'dkg-node'?: string;
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
  beneficiary: string;
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
  | 'AnchorOverDKG'
  | 'Bridge'
  | 'GovernanceBravoDelegate';
type RuntimeKind = 'DKG' | 'WebbProtocol';
type PalletKind = 'DKGProposals' | 'DKGProposalHandler';
