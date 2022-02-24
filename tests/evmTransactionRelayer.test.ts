// This our basic EVM Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just relay transactions for us.

import { jest } from '@jest/globals';
import 'jest-extended';
import { Anchors, Bridges, Tokens } from '@webb-tools/protocol-solidity';
import { ethers } from 'ethers';
import temp from 'temp';
import getPort, { portNumbers } from 'get-port';
import { LocalChain } from './lib/localTestnet';
import { WebbRelayer } from './lib/webbRelayer';

describe('EVM Transaction Relayer', () => {
  const tmp = temp.track();
  jest.setTimeout(40_000);
  const tmpDirPath = tmp.mkdirSync({ prefix: 'webb-relayer-test-' });
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureBridge: Bridges.SignatureBridge;

  let wallet1 = new ethers.Wallet(
    '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7e'
  );
  let wallet2 = new ethers.Wallet(
    '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7f'
  );

  let webbRelayer: WebbRelayer;

  beforeAll(async () => {
    // first we need to start local evm node.
    const localChain1Port = await getPort({ port: portNumbers(3333, 4444) });
    localChain1 = new LocalChain('TestA', localChain1Port, [
      {
        secretKey: wallet1.privateKey,
        balance: ethers.utils.parseEther('1000').toHexString(),
      },
    ]);

    const localChain2Port = await getPort({ port: portNumbers(3333, 4444) });
    localChain2 = new LocalChain('TestB', localChain2Port, [
      {
        secretKey: wallet2.privateKey,
        balance: ethers.utils.parseEther('1000').toHexString(),
      },
    ]);

    wallet1 = wallet1.connect(localChain1.provider());
    wallet2 = wallet2.connect(localChain2.provider());
    // Deploy the token.
    const localToken1 = await localChain2.deployToken(
      'Webb Token',
      'WEBB',
      wallet1
    );
    const localToken2 = await localChain2.deployToken(
      'Webb Token',
      'WEBB',
      wallet2
    );

    signatureBridge = await localChain1.deploySignatureBridge(
      localChain2,
      localToken1,
      localToken2,
      wallet1,
      wallet2
    );
    // save the chain configs.
    await localChain1.writeConfig(
      `${tmpDirPath}/${localChain1.name}.json`,
      signatureBridge
    );
    await localChain2.writeConfig(
      `${tmpDirPath}/${localChain2.name}.json`,
      signatureBridge
    );

    // get the anhor on localchain1
    const anchor = signatureBridge.getAnchor(
      localChain1.chainId,
      ethers.utils.parseEther('1')
    )!;
    await anchor.setSigner(wallet1);
    // approve token spending
    const tokenAddress = signatureBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    await token.approveSpending(anchor.contract.address);
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));

    // do the same but on localchain2
    const anchor2 = signatureBridge.getAnchor(
      localChain2.chainId,
      ethers.utils.parseEther('1')
    )!;
    await anchor2.setSigner(wallet2);
    const tokenAddress2 = signatureBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      wallet2
    );

    await token2.approveSpending(anchor2.contract.address);
    await token2.mintTokens(wallet2.address, ethers.utils.parseEther('1000'));

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
    });
    await webbRelayer.waitUntilReady();
  });

  test('Same Chain Relay Transaction', async () => {
    // we will use chain1 as an example here.
    const anchor1 = signatureBridge.getAnchor(
      localChain1.chainId,
      ethers.utils.parseEther('1')
    )! as Anchors.Anchor;

    // create a random account
    const sender = ethers.Wallet.createRandom().connect(localChain1.provider());
    // send them some ether.
    const tx1 = await wallet1.sendTransaction({
      to: sender.address,
      value: ethers.utils.parseEther('10'),
    });
    expect(tx1.wait()).toResolve();
    const relayerInfo = await webbRelayer.info();
    const localChain1Info = relayerInfo.evm[localChain1.chainId];
    const relayerFeePercentage =
      localChain1Info?.contracts.find(
        (c) => c.address === anchor1.contract.address
      )?.withdrawFeePercentage ?? 0n;
    // now we are ready to do the deposit.
    anchor1.setSigner(sender);
    const depositInfo = await anchor1.deposit(localChain1.chainId);

    // now we need to generate the proof and send it to the relayer!.
    const recipient = ethers.Wallet.createRandom().connect(
      localChain1.provider()
    );
    const withdrawalInfo = await anchor1.setupWithdraw(
      depositInfo.deposit,
      depositInfo.index,
      recipient.address,
      wallet1.address,
      relayerFeePercentage as bigint,
      0
    );

    // ping the relayer!
    await webbRelayer.ping();
    // now send the withdrawal request.
    const txHash = await webbRelayer.anchorWithdraw(
      localChain1.chainId.toString(),
      anchor1.getAddress(),
      withdrawalInfo.proofEncoded,
      //@ts-ignore
      withdrawalInfo.args
    );
    anchor1.setSigner(wallet1);
    expect(txHash).toBeDefined();
  });

  afterAll(async () => {
    await localChain1.stop();
    await localChain2.stop();
    await webbRelayer.stop();
    tmp.cleanupSync(); // clean up the temp dir.
  });
});
