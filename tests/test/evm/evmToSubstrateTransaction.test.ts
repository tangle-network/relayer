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
// This is evm to substrate cross transaction relayer tests.
// In this test we will deposit on evm vanchor system
// and withdraw through substrate vanchor system.


import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import isCi from 'is-ci';
import child from 'child_process';
import { ethers } from 'ethers';
import {
  WebbRelayer,
  Pallet,
  EnabledContracts,
} from '../../lib/webbRelayer.js';
import { LocalTangle } from '../../lib/localTangle.js';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import { naclEncrypt, randomAsU8a } from '@polkadot/util-crypto';

import {
  ProvingManagerSetupInput,
  ArkworksProvingManager,
  Utxo,
  calculateTypedChainId,
  ChainType,
  ProposalHeader,
  CircomUtxo,
  LeafIdentifier,
  Note,
} from '@webb-tools/sdk-core';

import {
  encodeResourceIdUpdateProposal,
  SubstrateResourceIdUpdateProposal,
} from '../../lib/substrateWebbProposals.js';
import pkg from 'secp256k1';
import { createSubstrateResourceId } from '../../lib/webbProposals.js';
import { LocalChain } from '../../lib/localTestnet.js';
import { Tokens, VBridge } from '@webb-tools/protocol-solidity';
import { expect } from 'chai';
import {
  createAccount,
  defaultEventsWatcherValue,
  generateVAnchorNote,
} from '../../lib/utils.js';
import { currencyToUnitI128, UsageMode } from '@webb-tools/test-utils';
import { VAnchor } from '@webb-tools/anchors';
import { MintableToken } from '@webb-tools/tokens';
import { KeyringPair } from '@polkadot/keyring/types.js';
const { ecdsaSign } = pkg;

describe('Cross chain transaction <<>> Mocked Backend', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let aliceNode: LocalTangle;
  let bobNode: LocalTangle;
  let webbRelayer: WebbRelayer;
  let wallet1: ethers.Wallet;
  let signatureVBridge: VBridge.VBridge;

  // Mnemonic seed phrase to created governor key used for signing proposals.
  // DO NOT USE IN PRODUCTION
  const mnemonic =
    'point shiver hurt flight fun online hub antenna engine pave chef fantasy front interest poem accident catch load frequent praise elite pet remove used';
  const governorWallet = ethers.Wallet.fromMnemonic(mnemonic.toString());
  const GOV = governorWallet.privateKey;
  const PK1 = u8aToHex(ethers.utils.randomBytes(32));
  // slice 0x04 from public key
  const uncompressedKey = governorWallet
    ._signingKey()
    .publicKey.toString()
    .slice(4);

  before(async () => {
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
        pallet: 'VAnchorBn254',
        eventsWatcher: defaultEventsWatcherValue,
      },
      {
        pallet: 'SignatureBridge',
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

    bobNode = await LocalTangle.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });

    // Wait until we are ready and connected
    const api = await aliceNode.api();
    await api.isReady;
    const substrateChainId = await aliceNode.getChainId();

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
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
        {
          secretKey: GOV,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
    });

    wallet1 = new ethers.Wallet(PK1, localChain1.provider());

    // Deploy the token.
    const localToken = await localChain1.deployToken('Webb Token', 'WEBB');

    const unwrappedToken = await MintableToken.createToken(
      'Webb Token',
      'WEBB',
      wallet1
    );

    signatureVBridge = await localChain1.deployVBridge(
      localToken,
      unwrappedToken,
      wallet1,
      governorWallet
    );

    // Get the anhor on localchain1
    const vanchor = signatureVBridge.getVAnchor(localChain1.chainId);

    await vanchor.setSigner(wallet1);

    const evmResourceId = await vanchor.createResourceId();

    const substrateResourceId = createSubstrateResourceId(
      substrateChainId,
      6,
      '0x2C'
    );

    // save the substrate chain configs
    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      chainId: substrateChainId,
      proposalSigningBackend: { type: 'Mocked', privateKey: GOV },
      linkedAnchors: [{ type: 'Raw', resourceId: evmResourceId }],
      enabledPallets,
    });

    // save the evm chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: {
        type: 'Mocked',
        privateKey: governorWallet.privateKey,
      },
      linkedAnchors: [
        { type: 'Raw', resourceId: substrateResourceId.toString() },
      ],
    });

    // This are pre-requisites for creating substrate chain.
    // 1. We need to set active governor on chain.
    // 2. We need to whitelist chain Id

    // force set maintainer
    const refreshNonce = 0;
    const setMaintainerCall = api.tx.signatureBridge.forceSetMaintainer(
      refreshNonce,
      `0x${uncompressedKey}`
    );
    await aliceNode.sudoExecuteTransaction(setMaintainerCall);
    const typedSourceChainId = calculateTypedChainId(
      ChainType.Substrate,
      substrateChainId
    );
    //whitelist chain
    const whitelistChainCall =
      api.tx.signatureBridge.whitelistChain(typedSourceChainId);
    await aliceNode.sudoExecuteTransaction(whitelistChainCall);

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(8000, 8888) });
    webbRelayer = new WebbRelayer({
      commonConfig: {
        port: relayerPort,
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
    });
    await webbRelayer.waitUntilReady();
  });

  it('Evm to Substrate cross chain transaction', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(wallet1);
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );

    const tx = await token.approveSpending(
      vanchor1.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();

    // Mint 1000 * 10^18 tokens to wallet1
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const api = await aliceNode.api();
    const account = createAccount('//Dave');
    // Create vanchor on Substrate chain with height 30 and maxEdges = 1
    const createVAnchorCall = api.tx.vAnchorBn254.create(1, 30, 0);
    await aliceNode.sudoExecuteTransaction(createVAnchorCall);
    const nextTreeId = await api.query.merkleTreeBn254.nextTreeId();
    const treeId = nextTreeId.toNumber() - 1;
    const substrateChainId = await aliceNode.getChainId();
    const typedTargetChainId = calculateTypedChainId(
      ChainType.Substrate,
      substrateChainId
    );
    const typedSourceChainId = localChain1.chainId;
    // now we set resource Id in the signature bridge
    const setResourceIdProposalCall = await setResourceIdProposal(
      api,
      GOV,
      treeId,
      substrateChainId
    );
    const txSigned = await setResourceIdProposalCall.signAsync(account);
    await aliceNode.executeTransaction(txSigned);

    // deposit amount on evm
    const publicAmount = currencyToUnitI128(10);
    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: publicAmount.toString(),
      originChainId: typedSourceChainId.toString(),
      chainId: typedTargetChainId.toString(),
    });

    const leaves = vanchor1.tree
      .elements()
      .map((el) => hexToU8a(el.toHexString()));

    await vanchor1.transact([], [depositUtxo], 0, 0, '0', '0', tokenAddress, {
      [typedSourceChainId]: leaves,
    });

    // now we wait for the proposal to be signed by mocked backend and then send data to signature bridge
    await webbRelayer.waitForEvent({
      kind: 'signing_backend',
      event: {
        backend: 'Mocked',
      },
    });

    // now we wait for proposals to be verified and executed by signature bridge through transaction queue.

    await webbRelayer.waitForEvent({
      kind: 'tx_queue',
      event: {
        ty: 'SUBSTRATE',
        chain_id: substrateChainId.toString(),
        finalized: true,
      },
    });
    // now we withdraw on substrate chain
    console.log('Withdraw on substrate');
    // dummy Deposit Note. Input note is directed toward source chain
    const depositNote = await generateVAnchorNote(
      0,
      typedTargetChainId,
      typedTargetChainId,
      0
    );
    // substrate vanchor withdraw
    await vanchorWithdraw(
      vanchor1,
      10,
      typedTargetChainId.toString(),
      depositNote,
      treeId,
      api,
      aliceNode
    );

    // now we wait for the proposal to be signed by mocked backend and then send data to signature bridge
    await webbRelayer.waitForEvent({
      kind: 'signing_backend',
      event: {
        backend: 'Mocked',
      },
    });

    // now we wait for proposals to be verified and executed by signature bridge through transaction queue.
    await webbRelayer.waitForEvent({
      kind: 'tx_queue',
      event: {
        ty: 'EVM',
        chain_id: localChain1.underlyingChainId.toString(),
        finalized: true,
      },
    });
  });

  after(async () => {
    await localChain1?.stop();
    await aliceNode?.stop();
    await bobNode?.stop();
    await webbRelayer?.stop();
  });
});

// Helper methods, we can move them somewhere if we end up using them again.

async function setResourceIdProposal(
  api: ApiPromise,
  PK1: string,
  treeId: number,
  chainId: number
): Promise<SubmittableExtrinsic<'promise'>> {
  const functionSignature = hexToU8a('0x00000002', 32);
  const nonce = 1;
  const palletIndex = '0x2C';
  const callIndex = '0x02';
  // set resource ID
  const resourceId = createSubstrateResourceId(chainId, treeId, palletIndex);
  const proposalHeader = new ProposalHeader(
    resourceId,
    functionSignature,
    nonce
  );
  const resourceIdUpdateProposal: SubstrateResourceIdUpdateProposal = {
    header: proposalHeader,
    newResourceId: resourceId.toString(),
    palletIndex,
    callIndex,
  };

  const proposalBytes = encodeResourceIdUpdateProposal(
    resourceIdUpdateProposal
  );
  const hash = ethers.utils.keccak256(proposalBytes);
  const msg = ethers.utils.arrayify(hash);
  // sign the message
  const sigObj = ecdsaSign(msg, hexToU8a(PK1));
  const signature = new Uint8Array([...sigObj.signature, sigObj.recid]);

  const setResourceCall = api.tx.signatureBridge.setResourceWithSignature(
    calculateTypedChainId(ChainType.Substrate, chainId),
    u8aToHex(proposalBytes),
    u8aToHex(signature)
  );
  return setResourceCall;
}

async function vanchorWithdraw(
  vanchor1: VAnchor,
  publicAmountUint: number,
  typedTargetChainId: string,
  depositNote: Note,
  treeId: number,
  api: ApiPromise,
  aliceNode: LocalTangle
) {
  const account = createAccount('//Dave');
  const typedSourceChainId = depositNote.note.sourceChainId;
  const secret = randomAsU8a();
  const gitRoot = child
    .execSync('git rev-parse --show-toplevel')
    .toString()
    .trim();

  const pkPath = path.join(
    // tests path
    gitRoot,
    'tests',
    'substrate-fixtures',
    'vanchor',
    'bn254',
    'x5',
    '2-2-2',
    'proving_key_uncompressed.bin'
  );
  const pk_hex = fs.readFileSync(pkPath).toString('hex');
  const pk = hexToU8a(pk_hex);

  const note1 = depositNote;
  const note2 = await note1.getDefaultUtxoNote();
  const publicAmount = currencyToUnitI128(publicAmountUint);
  const notes = [note1, note2];
  // Output UTXOs configs
  const output1 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: publicAmount.toString(),
    chainId: typedTargetChainId,
  });
  const output2 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: '0',
    chainId: typedTargetChainId,
  });

  // Configure a new proving manager with direct call
  const provingManager = new ArkworksProvingManager(null);
  const leavesMap = {};
  const assetId = new Uint8Array([0, 0, 0, 0]); // WEBB native token asset Id.
  const address = account.address;
  const extAmount = currencyToUnitI128(10);
  const fee = 0;
  const refund = 0;
  // get source chain (evm) leaves.
  const leaves = vanchor1.tree
    .elements()
    .map((el) => hexToU8a(el.toHexString()));
  leavesMap[typedSourceChainId.toString()] = leaves;

  const tree = await api.query.merkleTreeBn254.trees(treeId);
  const root = tree.unwrap().root.toHex();
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore TODO: Fix the issue with rpc type augmentation
  const neighborRoots: string[] = await api.rpc.lt
    .getNeighborRoots(treeId)
    .then((roots) => roots.toHuman());
  const rootsSet = [hexToU8a(root), hexToU8a(neighborRoots[0])];
  const decodedAddress = decodeAddress(address);
  const { encrypted: comEnc1 } = naclEncrypt(output1.commitment, secret);
  const { encrypted: comEnc2 } = naclEncrypt(output2.commitment, secret);
  const leafId: LeafIdentifier = {
    index: 0,
    typedChainId: Number(typedSourceChainId.toString()),
  };

  const setup: ProvingManagerSetupInput<'vanchor'> = {
    chainId: typedTargetChainId.toString(),
    leafIds: [leafId, leafId],
    inputUtxos: notes.map((n) => new Utxo(n.note.getUtxo())),
    leavesMap: leavesMap,
    output: [output1, output2],
    encryptedCommitments: [comEnc1, comEnc2],
    provingKey: pk,
    publicAmount: String(publicAmount),
    roots: rootsSet,
    relayer: decodedAddress,
    recipient: decodedAddress,
    extAmount: extAmount.toString(),
    fee: fee.toString(),
    refund: String(refund),
    token: assetId,
  };

  const data = await provingManager.prove('vanchor', setup);
  const extData = {
    relayer: address,
    recipient: address,
    fee,
    refund: String(refund),
    token: assetId,
    extAmount: extAmount.toNumber(),
    encryptedOutput1: u8aToHex(comEnc1),
    encryptedOutput2: u8aToHex(comEnc2),
  };
  const vanchorProofData = {
    proof: `0x${data.proof}`,
    publicAmount: data.publicAmount,
    roots: rootsSet,
    inputNullifiers: data.inputUtxos.map((input) => `0x${input.nullifier}`),
    outputCommitments: data.outputUtxos.map((utxo) => utxo.commitment),
    extDataHash: data.extDataHash,
  };

  // now we call the vanchor transact to withdraw on substrate
  const transactCall = api.tx.vAnchorBn254.transact(
    treeId,
    vanchorProofData,
    extData
  );
  const txSigned = await transactCall.signAsync(account);
  await aliceNode.executeTransaction(txSigned);
}
