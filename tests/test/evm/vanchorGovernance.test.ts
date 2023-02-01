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

import { expect } from 'chai';
import { Tokens, VBridge } from '@webb-tools/protocol-solidity';
import { CircomUtxo } from '@webb-tools/sdk-core';
import { BigNumber, ethers } from 'ethers';
import temp from 'temp';
import { LocalChain } from '../../lib/localTestnet.js';
import { EnabledContracts, WebbRelayer } from '../../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { u8aToHex, hexToU8a } from '@polkadot/util';

describe('VAnchor Governance Relayer', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureVBridge: VBridge.VBridge;
  let wallet1: ethers.Wallet;
  let wallet2: ethers.Wallet;

  let webbRelayer: WebbRelayer;

  before(async () => {
    const PK1 = u8aToHex(ethers.utils.randomBytes(32));
    const PK2 = u8aToHex(ethers.utils.randomBytes(32));
    const GOV = u8aToHex(ethers.utils.randomBytes(32));
    const localChain1Port = await getPort({
      port: portNumbers(3333, 4444),
    });

    const enabledContracts: EnabledContracts[] = [
      {
        contract: 'VAnchor',
      },
    ];

    localChain1 = await LocalChain.init({
      port: localChain1Port,
      chainId: localChain1Port,
      name: 'Hermes',
      populatedAccounts: [
        {
          secretKey: PK1,
          balance: ethers.utils.parseEther('1000').toHexString(),
        },
        {
          secretKey: GOV,
          balance: ethers.utils.parseEther('1000').toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
    });

    const localChain2Port = await getPort({
      port: portNumbers(3333, 4444),
    });

    localChain2 = await LocalChain.init({
      port: localChain2Port,
      chainId: localChain2Port,
      name: 'Athena',
      populatedAccounts: [
        {
          secretKey: PK2,
          balance: ethers.utils.parseEther('1000').toHexString(),
        },
        {
          secretKey: GOV,
          balance: ethers.utils.parseEther('1000').toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
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

    const govWallet = new ethers.Wallet(GOV);
    signatureVBridge = await localChain1.deploySignatureVBridge(
      localChain2,
      localToken1,
      localToken2,
      wallet1,
      wallet2,
      {
        [localChain1.chainId]: govWallet.address,
        [localChain2.chainId]: govWallet.address,
      }
    );

    const governorAddress = govWallet.address;
    const sides = signatureVBridge.vBridgeSides.values();
    for (const signatureSide of sides) {
      // check that the new governor is the same as the one we just set.
      const currentGovernor = await signatureSide.contract.governor();
      expect(currentGovernor).to.eq(governorAddress);
    }
    // get the anhor on localchain1
    const vanchor = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor.setSigner(wallet1);
    // approve token spending
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    let tx = await token.approveSpending(
      vanchor.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));

    // do the same but on localchain2
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor2.setSigner(wallet2);
    const tokenAddress2 = signatureVBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      wallet2
    );

    tx = await token2.approveSpending(
      vanchor2.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();
    await token2.mintTokens(wallet2.address, ethers.utils.parseEther('1000'));

    const evmResourceId1 = await vanchor.createResourceId();
    const evmResourceId2 = await vanchor2.createResourceId();
    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: GOV },
      linkedAnchors: [{ type: 'Raw', resourceId: evmResourceId2 }],
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: GOV },
      linkedAnchors: [{ type: 'Raw', resourceId: evmResourceId1 }],
    });

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      commonConfig: {
        port: relayerPort,
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: false,
      verbosity: 3,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should handle AnchorUpdateProposal when a deposit happens', async () => {
    // we will use chain1 as an example here.
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor1.setSigner(wallet1);
    await vanchor2.setSigner(wallet2);
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: '1',
      originChainId: localChain1.chainId.toString(),
      chainId: localChain1.chainId.toString(),
    });

    const leaves = vanchor1.tree
      .elements()
      .map((el) => hexToU8a(el.toHexString()));

    await vanchor1.transact([], [depositUtxo], 0, 0, '0', '0', tokenAddress, {
      [localChain1.chainId]: leaves,
    });
    // wait until the signature bridge recives the execute call.
    await webbRelayer.waitForEvent({
      kind: 'signature_bridge',
      event: { chain_id: localChain2.underlyingChainId.toString() },
    });
    // now we wait for the tx queue on that chain to execute the transaction.
    await webbRelayer.waitForEvent({
      kind: 'tx_queue',
      event: {
        ty: 'EVM',
        chain_id: localChain2.underlyingChainId.toString(),
        finalized: true,
      },
    });
    // all is good, last thing is to check for the roots.
    const srcChainRoot = await vanchor1.contract.getLastRoot();
    const neigborRoots = await vanchor2.contract.getLatestNeighborRoots();
    const edges = await vanchor2.contract.getLatestNeighborEdges();
    const isKnownNeighborRoot = neigborRoots.some(
      (root: BigNumber) => root.toHexString() === srcChainRoot.toHexString()
    );
    if (!isKnownNeighborRoot) {
      console.log({
        srcChainRoot,
        neigborRoots,
        edges,
        isKnownNeighborRoot,
      });
    }
    expect(isKnownNeighborRoot).to.be.true;
  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
