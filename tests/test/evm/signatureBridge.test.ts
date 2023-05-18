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
// A simple test for the Signature Bridge with the Relayer and the DKG.

import Chai, { expect } from 'chai';
import ChaiAsPromised from 'chai-as-promised';
import { Tokens, VBridge } from '@webb-tools/protocol-solidity';
import { hexToU8a, u8aToHex } from '@polkadot/util';
import { BigNumber, ethers } from 'ethers';
import temp from 'temp';
import retry from 'async-retry';
import { LocalChain } from '../../lib/localTestnet.js';
import {
  Pallet,
  WebbRelayer,
  EnabledContracts,
} from '../../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { LocalTangle } from '../../lib/localTangle.js';
import isCi from 'is-ci';
import path from 'path';
import { ethAddressFromUncompressedPublicKey } from '../../lib/ethHelperFunctions.js';
import { CircomUtxo, Keypair } from '@webb-tools/sdk-core';
import { UsageMode } from '@webb-tools/test-utils';
import { defaultEventsWatcherValue } from '../../lib/utils.js';
import { MintableToken } from '@webb-tools/tokens';

// to support chai-as-promised
Chai.use(ChaiAsPromised);

describe('Signature Bridge <> DKG Proposal Signing Backend', function () {
  this.timeout(5 * 60 * 1000);
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureBridge: VBridge.VBridge;
  let wallet1: ethers.Wallet;
  let wallet2: ethers.Wallet;
  let relayerWallet: ethers.Wallet;

  // dkg nodes
  let aliceNode: LocalTangle;
  let charlieNode: LocalTangle;

  let webbRelayer: WebbRelayer;

  before(async function () {
    const PK1 = u8aToHex(ethers.utils.randomBytes(32));
    const PK2 = u8aToHex(ethers.utils.randomBytes(32));
    const relayerPk = u8aToHex(ethers.utils.randomBytes(32));

    const usageMode: UsageMode = isCi
      ? { mode: 'docker', forcePullImage: false }
      : {
          mode: 'host',
          nodePath: path.resolve(
            '../../tangle/target/release/tangle-standalone'
          ),
        };
    const enabledPallets: Pallet[] = [
      {
        pallet: 'DKGProposalHandler',
        eventsWatcher: defaultEventsWatcherValue,
      },
      {
        pallet: 'DKG',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];
    aliceNode = await LocalTangle.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });

    charlieNode = await LocalTangle.start({
      name: 'substrate-charlie',
      authority: 'charlie',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });
    const api = await charlieNode.api();
    await api.isReady;
    console.log(
      'tangle node ready waiting for dkg public key to be set onchain'
    );
    const chainId = await charlieNode.getChainId();
    await charlieNode.writeConfig(`${tmpDirPath}/${charlieNode.name}.json`, {
      suri: '//Charlie',
      chainId: chainId,
      enabledPallets,
    });

    // we need to wait until the public key is on chain.
    await charlieNode.waitForEvent({
      section: 'dkg',
      method: 'PublicKeySignatureChanged',
    });

    // next we need to start local evm node.
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
          secretKey: relayerPk,
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
          secretKey: relayerPk,
          balance: ethers.utils.parseEther('1000').toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
    });

    wallet1 = new ethers.Wallet(PK1, localChain1.provider());
    wallet2 = new ethers.Wallet(PK2, localChain2.provider());
    relayerWallet = new ethers.Wallet(relayerPk, localChain1.provider());

    // Deploy the token.
    const wrappedToken1 = await localChain1.deployToken(
      'Wrapped Ethereum',
      'WETH'
    );
    const wrappedToken2 = await localChain2.deployToken(
      'Wrapped Ethereum',
      'WETH'
    );
    const unwrappedToken1 = await MintableToken.createToken(
      'Webb Token',
      'WEBB',
      wallet1
    );
    const unwrappedToken2 = await MintableToken.createToken(
      'Webb Token',
      'WEBB',
      wallet2
    );

    signatureBridge = await localChain1.deploySignatureVBridge(
      localChain2,
      wrappedToken1,
      wrappedToken2,
      wallet1,
      wallet2,
      unwrappedToken1,
      unwrappedToken2
    );

    // fetch the dkg public key.
    const dkgPublicKey = await charlieNode.fetchDkgPublicKey();
    expect(dkgPublicKey).to.not.equal('0x');
    const governorAddress = ethAddressFromUncompressedPublicKey(dkgPublicKey);
    // verify the governor address is a valid ethereum address.
    expect(ethers.utils.isAddress(governorAddress)).to.be.true;
    // transfer ownership to the DKG.
    const sides = signatureBridge.vBridgeSides.values();
    for (const signatureSide of sides) {
      // now we transferOwnership, forcefully.
      const tx = await signatureSide.transferOwnership(governorAddress, 1);
      await retry(
        async () => {
          await tx.wait();
        },
        {
          retries: 5,
          minTimeout: 1000,
          onRetry: (error) => {
            console.error('transferOwnership retry', error.name, error.message);
          },
        }
      );
      // check that the new governor is the same as the one we just set.
      const currentGovernor = await signatureSide.contract.governor();
      expect(currentGovernor).to.eq(governorAddress);
    }

    // get the anhor on localchain1
    const anchor = signatureBridge.getVAnchor(localChain1.chainId);
    await anchor.setSigner(wallet1);
    // approve token spending
    const tokenAddress = signatureBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    let tx = await token.approveSpending(
      anchor.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));

    // do the same but on localchain2
    const anchor2 = signatureBridge.getVAnchor(localChain2.chainId);
    await anchor2.setSigner(wallet2);
    const tokenAddress2 = signatureBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      wallet2
    );

    tx = await token2.approveSpending(
      anchor2.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();
    await token2.mintTokens(wallet2.address, ethers.utils.parseEther('1000'));

    const resourceId1 = await anchor.createResourceId();
    const resourceId2 = await anchor2.createResourceId();

    const setResourceIdCall = (resourceId: string) =>
      api.tx.dkgProposals.setResource(resourceId, '0x00');
    // register the resource on tangle node.
    for (const rid of [resourceId1, resourceId2]) {
      await charlieNode.sudoExecuteTransaction(setResourceIdCall(rid));
    }
    // Whitelist chainId on tangle node
    const whitelistChainIdCall = (chain_id: number) =>
      api.tx.dkgProposals.whitelistChain({ Evm: chain_id });
    for (const chain_id of [
      localChain1.underlyingChainId,
      localChain2.underlyingChainId,
    ]) {
      await charlieNode.sudoExecuteTransaction(whitelistChainIdCall(chain_id));
    }

    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge: signatureBridge,
      proposalSigningBackend: { type: 'DKGNode', chainId },
      linkedAnchors: [{ type: 'Raw', resourceId: resourceId2 }],
      relayerWallet,
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge: signatureBridge,
      proposalSigningBackend: { type: 'DKGNode', chainId },
      linkedAnchors: [{ type: 'Raw', resourceId: resourceId1 }],
      relayerWallet,
    });

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      commonConfig: {
        features: { governanceRelay: true },
        port: relayerPort,
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
      verbosity: 3,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should handle AnchorUpdateProposal when a deposit happens using DKG proposal backend', async () => {
    // we need to wait until the public key is changed.
    await charlieNode.waitForEvent({
      section: 'dkg',
      method: 'PublicKeySignatureChanged',
    });
    // wait until the signature bridge receives the transfer ownership call.
    await webbRelayer.waitForEvent({
      kind: 'signature_bridge',
      event: {
        chain_id: localChain2.underlyingChainId.toString(),
        call: 'transfer_ownership_with_signature_pub_key',
      },
    });

    // now we wait for the tx queue on that chain to execute the transfer ownership transaction.
     await webbRelayer.waitForEvent({
      kind: 'tx_queue',
      event: {
        ty: 'EVM',
        chain_id: localChain2.underlyingChainId.toString(),
        finalized: true,
      },
    });

    webbRelayer.clearLogs();
    // we will use chain1 as an example here.
    const anchor1 = signatureBridge.getVAnchor(localChain1.chainId);
    const anchor2 = signatureBridge.getVAnchor(localChain2.chainId);
    await anchor1.setSigner(wallet1);
    const tokenAddress = signatureBridge.getWebbTokenAddress(
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

    // Create a deposit utxo
    const randomKeypair = new Keypair();
    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // Make the deposit transaction
    const leaves = anchor1.tree
      .elements()
      .map((el) => hexToU8a(el.toHexString()));

    await anchor1.transact([], [depositUtxo], 0, 0, '0', '0', tokenAddress, {
      [localChain1.chainId]: leaves,
    });

    // now we wait for the proposal to be signed.
    charlieNode.waitForEvent({
      section: 'dkgProposalHandler',
      method: 'ProposalSigned',
    });

    // wait until the signature bridge receives the execute call.
    await webbRelayer.waitForEvent({
      kind: 'signature_bridge',
      event: {
        chain_id: localChain2.underlyingChainId.toString(),
        call: 'execute_proposal_with_signature',
      },
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
    const srcChainRoot = await anchor1.contract.getLastRoot();
    const neigborRoots = await anchor2.contract.getLatestNeighborRoots();
    const edges = await anchor2.contract.getLatestNeighborEdges();
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
    await aliceNode?.stop();
    await charlieNode?.stop();
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
