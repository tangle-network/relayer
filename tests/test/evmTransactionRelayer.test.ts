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
// This our basic EVM Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just relay transactions for us.

import { expect } from 'chai';
import { Bridges, Tokens } from '@webb-tools/protocol-solidity';
import { ethers } from 'ethers';
import temp from 'temp';
import { LocalChain } from '../lib/localTestnet.js';
import { calcualteRelayerFees, WebbRelayer } from '../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { IAnchor } from '@webb-tools/interfaces';
import { IAnchorDeposit } from '@webb-tools/interfaces/src/anchor/index';
import { u8aToHex } from '@polkadot/util';

describe('EVM Transaction Relayer', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureBridge: Bridges.SignatureBridge;
  let wallet1: ethers.Wallet;
  let wallet2: ethers.Wallet;

  let webbRelayer: WebbRelayer;

  before(async () => {
    const PK1 = u8aToHex(ethers.utils.randomBytes(32));
    const PK2 = u8aToHex(ethers.utils.randomBytes(32));
    // first we need to start local evm node.
    const localChain1Port = await getPort({
      port: portNumbers(3333, 4444),
    });
    localChain1 = new LocalChain({
      port: localChain1Port,
      chainId: 5001,
      name: 'Hermes',
      populatedAccounts: [
        {
          secretKey: PK1,
          balance: ethers.utils.parseEther('100000').toHexString(),
        },
      ],
    });

    const localChain2Port = await getPort({
      port: portNumbers(3333, 4444),
    });
    localChain2 = new LocalChain({
      port: localChain2Port,
      chainId: 5002,
      name: 'Athena',
      populatedAccounts: [
        {
          secretKey: PK2,
          balance: ethers.utils.parseEther('100000').toHexString(),
        },
      ],
    });

    wallet1 = new ethers.Wallet(PK1, localChain1.provider());
    wallet2 = new ethers.Wallet(PK2, localChain2.provider());
    // Deploy the token.
    const localToken1 = await localChain1.deployToken(
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
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK2 },
    });

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
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('5'));

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
    await token2.mintTokens(wallet2.address, ethers.utils.parseEther('5'));

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
      showLogs: false,
    });
    await webbRelayer.waitUntilReady();
  });

  it('number of deposits made should be equal to number of leaves in cache', async () => {
    this.retries(0);
    let anchor1 = await setUpAnchor(signatureBridge, localChain1.chainId);

    // set signer
    await anchor1.setSigner(wallet1);
    const tokenAddress = signatureBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token

    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));
    // check webbBalance
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;
    // Make multiple deposits
    const noOfDeposit = 5;
    for (let i = 0, len = noOfDeposit; i < len; i++) {
      await anchor1.deposit(localChain2.chainId);
    }
    // now we wait for all deposit to be saved in LeafStorageCache
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: (noOfDeposit - 1).toString(),
      },
    });

    // now we call relayer leaf API to check no of leaves stored in LeafStorageCache
    // are equal to no of deposits made.
    const chainId = localChain1.underlyingChainId.toString(16);
    const response = await webbRelayer.getLeaves(
      chainId,
      anchor1.contract.address
    );
    expect(noOfDeposit).to.equal(response.leaves.length);
  });

  it('should relay same transaction on same chain', async () => {
    // we will use chain1 as an example here.
    let anchor1 = await setUpAnchor(signatureBridge, localChain1.chainId);

    await anchor1.setSigner(wallet1);
    const tokenAddress = signatureBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    // mint tokins to the account everytime.
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('5'));
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;
    // now we are ready to do the deposit.
    const depositInfo = await anchor1.deposit(localChain1.chainId);
    const recipient = new ethers.Wallet(
      ethers.utils.randomBytes(32),
      localChain1.provider()
    );

    const relayerInfo = await webbRelayer.info();
    const localChain1Info = relayerInfo.evm[localChain1.underlyingChainId];
    const relayerFeePercentage =
      localChain1Info?.contracts.find(
        (c) => c.address === anchor1.contract.address
      )?.withdrawFeePercentage ?? 0;

    // check balance of recipient before withdrawal
    let webbBalanceOfRecipient = await token.getBalance(recipient.address);
    let initialBalanceOfRecipient = webbBalanceOfRecipient.toBigInt();

    const { args, publicInputs, extData } = await anchor1.setupWithdraw(
      depositInfo.deposit,
      depositInfo.index,
      recipient.address,
      wallet1.address,
      calcualteRelayerFees(
        anchor1.denomination!,
        relayerFeePercentage
      ).toBigInt(),
      0
    );
    const [proofEncoded, roots, nullifierHash, extDataHash] = args;
    // ping the relayer!
    await webbRelayer.ping();
    // now send the withdrawal request.
    const txHash = await webbRelayer.anchorWithdraw(
      localChain1.underlyingChainId.toString(),
      anchor1.getAddress(),
      proofEncoded,
      publicInputs,
      extData
    );
    expect(txHash).to.be.string;

    webbBalanceOfRecipient = await token.getBalance(recipient.address);
    let balanceOfRecipientAfterWithdraw = webbBalanceOfRecipient.toBigInt();

    // check that recipient balance has increased
    expect(balanceOfRecipientAfterWithdraw > initialBalanceOfRecipient);
  });

  it('Should fail to withdraw if address is invalid', async () => {
    // we will use chain1 as an example here.
    let anchor1 = await setUpAnchor(signatureBridge, localChain1.chainId);
    let depositInfo = await makeDeposit(
      signatureBridge,
      anchor1,
      wallet1,
      localChain1.chainId
    );

    const [proofEncoded, publicInputs, extData] = await initWithdrawal(
      localChain1,
      webbRelayer,
      anchor1,
      wallet1,
      depositInfo
    );

    // now send the withdrawal request with a wrong recipient address
    try {
      await webbRelayer.anchorWithdraw(
        localChain1.underlyingChainId.toString(),
        wallet2.address,
        proofEncoded,
        publicInputs,
        extData
      );
    } catch (e) {
      expect(e).to.not.be.null;
      expect(e).to.be.eq(`unsupportedContract`);
    }
  });

  it('Should fail to withdraw if proof is invalid', async () => {
    // we will use chain1 as an example here.
    let anchor1 = await setUpAnchor(signatureBridge, localChain1.chainId);
    let depositInfo = await makeDeposit(
      signatureBridge,
      anchor1,
      wallet1,
      localChain1.chainId
    );

    const [proofEncoded, publicInputs, extData] = await initWithdrawal(
      localChain1,
      webbRelayer,
      anchor1,
      wallet1,
      depositInfo
    );

    const invalidProof = '0xef4b4f4d7554be477e828636a4e69b3f44d18ec0';

    // now send the withdrawal request with a wrong recipient address
    try {
      await webbRelayer.anchorWithdraw(
        localChain1.underlyingChainId.toString(),
        anchor1.getAddress(),
        invalidProof,
        publicInputs,
        extData
      );
    } catch (e) {
      expect(e).to.not.be.null;
      expect(JSON.stringify(e)).to.contain(
        `VM Exception while processing transaction`
      );
    }
  });

  it('Should fail to withdraw if fee is not expected', async () => {
    // we will use chain1 as an example here.
    let anchor1 = await setUpAnchor(signatureBridge, localChain1.chainId);
    let depositInfo = await makeDeposit(
      signatureBridge,
      anchor1,
      wallet1,
      localChain1.chainId
    );

    const [proofEncoded, publicInputs, extData] = await initWithdrawal(
      localChain1,
      webbRelayer,
      anchor1,
      wallet1,
      depositInfo
    );

    extData.fee = 100;
    // now send the withdrawal request with a wrong recipient address
    try {
      await webbRelayer.anchorWithdraw(
        localChain1.underlyingChainId.toString(),
        anchor1.getAddress(),
        proofEncoded,
        publicInputs,
        extData
      );
    } catch (e) {
      expect(e).to.not.be.null;
      expect(e).to.be.eq(`unsupportedContract`);
    }
  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});

async function setUpAnchor(
  signatureBridge: Bridges.SignatureBridge,
  chainId: number
): Promise<any> {
  const anchor1 = signatureBridge.getAnchor(
    chainId,
    ethers.utils.parseEther('1')
  );

  return anchor1;
}
async function makeDeposit(
  signatureBridge: Bridges.SignatureBridge,
  anchor: IAnchor,
  wallet: ethers.Wallet,
  chainId: number
): Promise<IAnchorDeposit> {
  await anchor.setSigner(wallet);
  const tokenAddress = signatureBridge.getWebbTokenAddress(chainId)!;
  const token = await Tokens.MintableToken.tokenFromAddress(
    tokenAddress,
    wallet
  );

  // mint tokins to the account everytime.
  await token.mintTokens(wallet.address, ethers.utils.parseEther('5'));
  const webbBalance = await token.getBalance(wallet.address);
  expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to.be
    .true;
  // now we are ready to do the deposit.
  const depositInfo = await anchor.deposit(chainId);

  return depositInfo;
}

async function initWithdrawal(
  localChain: LocalChain,
  webbRelayer: WebbRelayer,
  anchor: IAnchor,
  wallet: ethers.Wallet,
  depositInfo: IAnchorDeposit
): Promise<any> {
  const recipient = new ethers.Wallet(
    ethers.utils.randomBytes(32),
    localChain.provider()
  );

  const relayerInfo = await webbRelayer.info();
  const localChain1Info = relayerInfo.evm[localChain.underlyingChainId];
  const relayerFeePercentage =
    localChain1Info?.contracts.find(
      (c) => c.address === anchor.contract.address
    )?.withdrawFeePercentage ?? 0;
  const { args, publicInputs, extData } = await anchor.setupWithdraw(
    depositInfo.deposit,
    depositInfo.index,
    recipient.address,
    wallet.address,
    calcualteRelayerFees(anchor.denomination!, relayerFeePercentage).toBigInt(),
    0
  );
  const [proofEncoded, roots, nullifierHash, extDataHash] = args;
  // ping the relayer!
  await webbRelayer.ping();

  return [proofEncoded, publicInputs, extData];
}
