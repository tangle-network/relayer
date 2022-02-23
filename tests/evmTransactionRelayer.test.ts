// This our basic EVM Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just relay transactions for us.

import { jest } from '@jest/globals';
import { ethers } from 'ethers';
import { LocalChain } from './lib/localTestnet';
import { WebbRelayer } from './lib/webbRelayer';

describe('EVM Transaction Relayer', () => {
  jest.setTimeout(20_000);
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  const wallet1 = new ethers.Wallet(
    '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7e'
  );
  const wallet2 = new ethers.Wallet(
    '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7e'
  );

  let webbRelayer: WebbRelayer;

  beforeAll(async () => {
    // first we need to start local evm node.
    localChain1 = new LocalChain('TestA', 3333, [
      {
        secretKey: wallet1.privateKey,
        balance: ethers.utils.parseEther('10').toHexString(),
      },
    ]);
    localChain2 = new LocalChain('TestB', 4444, [
      {
        secretKey: wallet2.privateKey,
        balance: ethers.utils.parseEther('10').toHexString(),
      },
    ]);

    webbRelayer = new WebbRelayer(9933);
    await webbRelayer.waitUntilReady();
  });

  test('it should relay transaction', async () => {
    // TBD.
    expect(true).toBe(true);
  });

  afterAll(async () => {
    await localChain1.stop();
    await localChain2.stop();
    await webbRelayer.stop();
  });
});
