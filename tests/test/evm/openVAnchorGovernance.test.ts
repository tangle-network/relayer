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

import { BigNumber } from 'ethers';
import { expect } from 'chai';
import { MintableToken } from '@webb-tools/tokens';
import { ethers } from 'ethers';
import temp from 'temp';
import { LocalChain } from '../../lib/localTestnetOpenVBridge.js';
import { EnabledContracts, WebbRelayer } from '../../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { OpenVBridge } from '@webb-tools/vbridge';

describe('Open VAnchor Governance Relayer', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureVBridge: OpenVBridge;
  let wallet1: ethers.Wallet;
  let wallet2: ethers.Wallet;
  let localToken1: MintableToken;
  let localToken2: MintableToken;

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
        contract: 'OpenVAnchor',
      },
    ];

    localChain1 = await LocalChain.init({
      port: localChain1Port,
      chainId: 5001,
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
      chainId: 5002,
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
    localToken1 = await localChain1.deployToken('Webb Token', 'WEBB', wallet1);
    localToken2 = await localChain2.deployToken('Webb Token', 'WEBB', wallet2);

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
    const openVAnchor = signatureVBridge.getVAnchor(localChain1.chainId);
    await openVAnchor.setSigner(wallet1);
    // approve token spending
    const token = await MintableToken.tokenFromAddress(
      localToken1.contract.address,
      wallet1
    );
    await token.mintTokens(wallet1.address, '1000000');

    // do the same but on localchain2
    const openVAnchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await openVAnchor2.setSigner(wallet2);
    const token2 = await MintableToken.tokenFromAddress(
      localToken2.contract.address,
      wallet2
    );
    await token2.mintTokens(wallet2.address, '1000000');
    const resourceId1 = await openVAnchor.createResourceId();
    const resourceId2 = await openVAnchor2.createResourceId();
    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: GOV },
      linkedAnchors: [{ type: 'Raw', resourceId: resourceId2 }],
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: GOV },
      linkedAnchors: [{ type: 'Raw', resourceId: resourceId1 }],
    });
    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      tmp: true,
      commonConfig: {
        port: relayerPort,
      },
      configDir: tmpDirPath,
      showLogs: false,
      verbosity: 4,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should handle AnchorUpdateProposal when a deposit happens', async () => {
    // we will use chain1 as an example here.
    const openVAnchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    const openVAnchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await openVAnchor2.setSigner(wallet2);
    const wrappedTokenAddress = await signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    console.log(wrappedTokenAddress, await openVAnchor1.contract.token());
    const token = await MintableToken.tokenFromAddress(
      localToken1.contract.address,
      wallet1
    );
    // Approve the wrapped token to spend the wrapping token.
    let tx = await token.contract.approve(wrappedTokenAddress, 1000000, {
      from: wallet1.address,
    });
    await tx.wait();

    const depositAmount = 100;
    const destChainId = localChain2.chainId;
    const recipient = await wallet1.getAddress();
    const delegatedCalldata = '0x00';
    const blinding = BigNumber.from(1010101010);
    await openVAnchor1.setSigner(wallet1);
    tx = await openVAnchor1.contract.wrapAndDeposit(
      destChainId,
      depositAmount,
      recipient,
      delegatedCalldata,
      blinding,
      BigNumber.from(1010101010),
      token.contract.address
    );
    await tx.wait();

    tx = await openVAnchor1.contract.wrapAndDeposit(
      destChainId,
      depositAmount,
      recipient,
      delegatedCalldata,
      blinding,
      BigNumber.from(1010101011),
      token.contract.address
    );
    await tx.wait();

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
    const srcChainRoot = await openVAnchor1.contract.getLastRoot();
    const neigborRoots = await openVAnchor2.contract.getLatestNeighborRoots();
    const edges = await openVAnchor2.contract.getLatestNeighborEdges();
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
