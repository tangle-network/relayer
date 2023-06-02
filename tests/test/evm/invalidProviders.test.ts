import http from 'node:http';
import fs from 'node:fs';
import getPort from 'get-port';
import temp from 'temp';
import Chai, { expect } from 'chai';
import ChaiAsPromised from 'chai-as-promised';

import { WebbRelayer } from '../../lib/webbRelayer.js';

// to support chai-as-promised
Chai.use(ChaiAsPromised);

/**
 * An Invalid Provider that always returns an error when queried.
 * It is used to test the error handling of the EVM.
 */
abstract class InvalidProvider {
  private readonly server: http.Server;

  constructor(public readonly port: number) {
    this.server = http
      .createServer((request, response) => this.handle(request, response))
      .listen(port);
  }

  public async destroy(): Promise<void> {
    await new Promise<void>((resolve, reject) => {
      this.server.close((error) => {
        if (error) {
          reject(error);
        } else {
          resolve();
        }
      });
    });
  }

  public get url(): string {
    return `http://localhost:${this.port}`;
  }

  protected abstract handle(
    request: http.IncomingMessage,
    response: http.ServerResponse
  ): void;
}

class RateLimitedProvider extends InvalidProvider {
  public get errorMessage(): string {
    return '<html><body><h1>429 Too Many Requests</h1>You have sent too many requests in a given amount of time.</body></html>';
  }

  protected handle(
    _request: http.IncomingMessage,
    response: http.ServerResponse
  ): void {
    response.writeHead(429, {
      'Content-Type': 'text/html',
      'X-Content-Type-Options': 'nosniff',
    });
    response.end(this.errorMessage);
  }
}

describe('Invalid EVM Providers', () => {
  describe('Rate Limited Provider', function () {
    const tmpDirPath = temp.mkdirSync();
    let provider: RateLimitedProvider;
    let webbRelayer: WebbRelayer;

    before(async function () {
      this.timeout(0);
      provider = new RateLimitedProvider(await getPort());

      const config = {
        evm: {
          1: {
            name: 'rate-limited-provider',
            enabled: true,
            'chain-id': 1,
            'http-endpoints': [provider.url],
            'ws-endpoint': 'ws://localhost:8546',
            'block-confirmations': 0,
            contracts: [
              {
                contract: 'VAnchor',
                address: '0x0000000000000000000000000000000000000000',
                'deployed-at': 0,
                'events-watcher': {
                  enabled: true,
                  'polling-interval': 100,
                  'print-progress-interval': 1000,
                  'linked-anchors': [],
                },
              },
            ],
          },
        },
      };

      const configString = JSON.stringify(config, null, 2);
      fs.writeFileSync(
        `${tmpDirPath}/rate-limited-provider.json`,
        configString
      );
      // now start the relayer
      const relayerPort = await getPort();
      webbRelayer = new WebbRelayer({
        commonConfig: {
          port: relayerPort,
        },
        tmp: true,
        configDir: tmpDirPath,
        showLogs: true,
        verbosity: 3,
      });
    });

    it('should keep retrying with the rate limited provider', async function () {
      await webbRelayer.waitForEvent({
        kind: 'retry',
        event: {
          should_retry: true,
          error: provider.errorMessage.toLowerCase(),
        },
      });
    });

    after(async () => {
      await webbRelayer.stop();
      await provider.destroy();
    });
  });
});
