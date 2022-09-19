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
// This is Substrate VAnchor Transaction Relayer Tests.
// In this test relayer on vanchor deposit will create and relay proposals to signature bridge pallet for execution

import '@nepoche/protocol-substrate-types';
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
import { LocalProtocolSubstrate } from '../../lib/localProtocolSubstrate.js';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import {
  UsageMode,
  defaultEventsWatcherValue,
} from '../../lib/substrateNodeBase.js';
import { BigNumber } from 'ethers';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import { naclEncrypt, randomAsU8a } from '@polkadot/util-crypto';
import { verify_js_proof } from '@webb-tools/wasm-utils/njs/wasm-utils-njs.js';
import {
  Note,
  ProvingManagerSetupInput,
  ArkworksProvingManager,
  Utxo,
  calculateTypedChainId,
  ChainType,
  toFixedHex,
  ResourceId,
  ProposalHeader,
  CircomUtxo,
  Keypair,
  randomBN,
} from '@webb-tools/sdk-core';

import {
  encodeResourceIdUpdateProposal,
  SubstrateResourceIdUpdateProposal,
} from '../../lib/substrateWebbProposals.js';
import pkg from 'secp256k1';
import { makeSubstrateTargetSystem } from '../../lib/webbProposals.js';
import { LocalChain } from '../../lib/localTestnet.js';
import { Tokens, VBridge } from '@webb-tools/protocol-solidity';
import { expect } from 'chai';
import { sleep } from '../../lib/sleep.js';
const { ecdsaSign } = pkg;

describe.only('Cross chain transaction <<>> Mocked Backend', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;
  let webbRelayer: WebbRelayer;
  let wallet1: ethers.Wallet;
  let signatureVBridge: VBridge.VBridge;

  // Governer key
  const GOV = u8aToHex(ethers.utils.randomBytes(32));
  const PK1 = u8aToHex(ethers.utils.randomBytes(32));
  
  let governorWallet = new ethers.Wallet(GOV);
  // slice 0x04 from public key
  let uncompressedKey = governorWallet
    ._signingKey()
    .publicKey.toString()
    .slice(4);

  before(async () => {
    const usageMode: UsageMode = isCi
      ? { mode: 'docker', forcePullImage: false }
      : {
          mode: 'host',
          nodePath: path.resolve(
            '../../protocol-substrate/target/release/webb-standalone-node'
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

    aliceNode = await LocalProtocolSubstrate.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enabledPallets,
      enableLogging: false,
    });

    bobNode = await LocalProtocolSubstrate.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });

    // Wait until we are ready and connected
    const api = await aliceNode.api();
    await api.isReady;
    let substrateChainId = await aliceNode.getChainId();

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

    wallet1 = new ethers.Wallet(PK1, localChain1.provider());
    // Deploy the token.
    const localToken1 = await localChain1.deployToken(
      'Webb Token',
      'WEBB',
      wallet1
    );

    signatureVBridge = await localChain1.deployVBridge(
      localToken1,
      wallet1,
      governorWallet
    );

    // get the anhor on localchain1
    const vanchor = signatureVBridge.getVAnchor(localChain1.chainId)!;
    await vanchor.setSigner(wallet1);

    let evmResourceId = (await vanchor.createResourceId()).slice(2);
    console.log('evm resourceId : ', evmResourceId);

    let substrateResourceId = createSubstrateResourceId(substrateChainId);
    console.log('substrate resourceId : ', substrateResourceId);
    // save the substrate chain configs
    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      chainId: substrateChainId,
      proposalSigningBackend: { type: 'Mocked', privateKey: GOV },
      linkedAnchors: [{ type: 'Raw', resourceId: `0x${evmResourceId}` }],
    });

    // save the evm chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: GOV },
      linkedAnchors: [{ type: 'Raw', resourceId: substrateResourceId }],
    });

    // This are pre-requisites for creating substrate chain.
    // 1. We need to set active governor on chain.
    // 2. We need to whitelist chain Id

    // force set maintainer
    let setMaintainerCall = api.tx.signatureBridge!.forceSetMaintainer!(
      `0x${uncompressedKey}`
    );
    await aliceNode.sudoExecuteTransaction(setMaintainerCall);
    let typedSourceChainId = calculateTypedChainId(
      ChainType.Substrate,
      substrateChainId
    );
    //whitelist chain
    let whitelistChainCall =
      api.tx.signatureBridge.whitelistChain(typedSourceChainId);
    await aliceNode.sudoExecuteTransaction(whitelistChainCall);

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(8000, 8888) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
    });
    await webbRelayer.waitUntilReady();
  });

  it('Substrate to Evm cross chain transaction', async () => {
    const amount1 = (1e13).toString();
    const amount2 = currencyToUnitI128(10).toString();
    console.log("amount1: ", amount1);
    console.log("amount2: ", amount2);
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(wallet1);

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

    const api = await aliceNode.api();
    const account = createAccount('//Dave');
    //create vanchor
    let createVAnchorCall = api.tx.vAnchorBn254!.create!(1, 30, 0);
    await aliceNode.sudoExecuteTransaction(createVAnchorCall);
    const nextTreeId = await api.query.merkleTreeBn254.nextTreeId();
    const treeId = nextTreeId.toNumber() - 1;
    console.log('treeId : ', treeId);
    // chainId
    let substrateChainId = await aliceNode.getChainId();
    let typedSourceChainId = calculateTypedChainId(
      ChainType.Substrate,
      substrateChainId
    );
    let typedTargetChainId = localChain1.chainId;
    console.log('typedSourceChainId : ', typedSourceChainId);
    // now we set resource through proposal execution
    let setResourceIdProposalCall = await setResourceIdProposal(
      api,
      GOV,
      treeId,
      substrateChainId
    );
    const txSigned = await setResourceIdProposalCall.signAsync(account);
    await aliceNode.executeTransaction(txSigned);
    
    // dummy Deposit Note. Input note is directed toward source chain
    const depositNote = await generateVAnchorNote(
      0,
      typedSourceChainId,
      typedSourceChainId,
      0
    );
    
    // substrate vanchor deposit
    let data = await vanchorDeposit(typedTargetChainId.toString(),depositNote, treeId, api, aliceNode);
    
    // expect()
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
    console.log('Withdraw on evm');

    // now we withdraw on evm chain
    const leaves = vanchor1.tree
      .elements()
      .map((el) => hexToU8a(el.toHexString()));
    const publicAmount = (1e13).toString();

    
    // get leaves for substrate chain
    //@ts-ignore
    const substrateLeaves = await api.derive.merkleTreeBn254.getLeavesForTree(6, 0, 1);
    console.log("susbtrate leaves : ", substrateLeaves);
    const evmChainRoot = await vanchor1.contract.getLastRoot();
    const neigborRoots = await vanchor1.contract.getLatestNeighborRoots();
    const edges = await vanchor1.contract.getLatestNeighborEdges();
    
    console.log("evmChainRoot : ",evmChainRoot);
    console.log("neigborRoots: ",neigborRoots);
    console.log("edges: ", edges);
    let index = substrateLeaves.findIndex((leaf) => data.outputUtxo.commitment.toString() === leaf.toString())
    console.log("index : ", index);
    console.log("outputnote: ", data.outputUtxo.serialize());
    const withdrawUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: publicAmount,
      originChainId: typedSourceChainId.toString(),
      chainId: typedTargetChainId.toString(),
      keypair: data.keyPair,
      index: index.toString()
    });
    console.log("withdrawal UTXO : ", withdrawUtxo.serialize());
    let res = await vanchor1.transact(
      [withdrawUtxo],
      [],
      {
        [localChain1.chainId]: leaves,
        [typedSourceChainId]: substrateLeaves
      },
      '0',
      '0',
      '0',
      '0'
    );
    console.log(res);
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
        ty: 'Substrate',
        chain_id: substrateChainId.toString(),
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
  let functionSignature = hexToU8a('0x00000002', 32);
  let nonce = 1;
  let palletIndex = '0x2D';
  let callIndex = '0x02';
  let substrateTargetSystem = makeSubstrateTargetSystem(treeId, palletIndex);
  // set resource ID
  let resourceId = new ResourceId(
    toFixedHex(substrateTargetSystem, 20),
    ChainType.Substrate,
    chainId
  );
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

  let proposalBytes = encodeResourceIdUpdateProposal(resourceIdUpdateProposal);
  let hash = ethers.utils.keccak256(proposalBytes);
  let msg = ethers.utils.arrayify(hash);
  // sign the message
  const sigObj = ecdsaSign(msg, hexToU8a(PK1));
  let signature = new Uint8Array([...sigObj.signature, sigObj.recid]);
  // execute proposal call to handler
  let executeSetProposalCall =
    api.tx.vAnchorHandlerBn254.executeSetResourceProposal(resourceId.toU8a());
  let setResourceCall = api.tx.signatureBridge!.setResourceWithSignature!(
    calculateTypedChainId(ChainType.Substrate, chainId),
    executeSetProposalCall,
    u8aToHex(proposalBytes),
    u8aToHex(signature)
  );
  return setResourceCall;
}

async function vanchorDeposit(
  typedTargetChainId:string,
  depositNote: Note,
  treeId: number,
  api: ApiPromise,
  aliceNode: LocalProtocolSubstrate
) : Promise<{outputUtxo: Utxo, keyPair:Keypair}>{
  const account = createAccount('//Dave');
  const typedSourceChainId = depositNote.note.sourceChainId;
  console.log('typedSourceChainId : ', typedSourceChainId);
  console.log('typedTargetChainId : ', typedTargetChainId);
  const secret = randomAsU8a();
  // Key pair for deposit UTXO for spend
  const randomKeypair = new Keypair();
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

  const vkPath = path.join(
    // tests path
    gitRoot,
    'tests',
    'substrate-fixtures',
    'vanchor',
    'bn254',
    'x5',
    '2-2-2',
    'verifying_key_uncompressed.bin'
  );
  const vk_hex = fs.readFileSync(vkPath).toString('hex');
  const vk = hexToU8a(vk_hex);

  let note1 = depositNote;
  const note2 = await note1.getDefaultUtxoNote();
  const publicAmount = currencyToUnitI128(10);
  const notes = [note1, note2];
  // Output UTXOs configs
  const output1 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: publicAmount.toString(),
    chainId: typedTargetChainId,
    keypair: randomKeypair,
  });
  const output2 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: '0',
    chainId: typedTargetChainId,

  });

  // Configure a new proving manager with direct call
  const provingManager = new ArkworksProvingManager(null);
  const leavesMap: any = {};

  const address = account.address;
  const extAmount = currencyToUnitI128(10);
  const fee = 0;
  const refund = 0;
  // Empty leaves
  leavesMap[typedTargetChainId.toString()] = [];
  const tree = await api.query.merkleTreeBn254.trees(treeId);
  const root = tree.unwrap().root.toHex();
  const neighborRoots: string[] = await (api.rpc as any).lt
    .getNeighborRoots(treeId)
    .then((roots: any) => roots.toHuman());
  console.log('neighbor roots: ', neighborRoots);
  const rootsSet = [hexToU8a(root), hexToU8a(neighborRoots[0])];
  const decodedAddress = decodeAddress(address);
  const { encrypted: comEnc1 } = naclEncrypt(output1.commitment, secret);
  const { encrypted: comEnc2 } = naclEncrypt(output2.commitment, secret);

  const setup: ProvingManagerSetupInput<'vanchor'> = {
    chainId: typedSourceChainId.toString(),
    indices: [0, 0],
    inputNotes: notes,
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
    token: decodedAddress,
  };

  const data = await provingManager.prove('vanchor', setup);
  const extData = {
    relayer: address,
    recipient: address,
    fee,
    refund: String(refund),
    token: decodedAddress,
    extAmount: extAmount,
    encryptedOutput1: u8aToHex(comEnc1),
    encryptedOutput2: u8aToHex(comEnc2),
  };

  let vanchorProofData = {
    proof: `0x${data.proof}`,
    publicAmount: data.publicAmount,
    roots: rootsSet,
    inputNullifiers: data.inputUtxos.map((input) => `0x${input.nullifier}`),
    outputCommitments: data.outputNotes.map((note) =>
      u8aToHex(note.note.getLeafCommitment())
    ),
    extDataHash: data.extDataHash,
  };

  console.log('Proof data : ', vanchorProofData);
  console.log("verify proof ");
  const isValidProof = verify_js_proof(data.proof, data.publicInputs, u8aToHex(vk).replace('0x', ''), 'Bn254');
  console.log("Is proof valid : ", isValidProof);
  //@ts-ignore
  const leafsCount = await api.derive.merkleTreeBn254.getLeafCountForTree(
    Number(treeId)
  );
  const indexBeforeInsetion = Math.max(leafsCount - 1, 0);

  // now we call the vanchor transact
  let transactCall = api.tx.vAnchorBn254!.transact!(
    treeId,
    vanchorProofData,
    extData
  );
  const txSigned = await transactCall.signAsync(account);
  await aliceNode.executeTransaction(txSigned);
  return { outputUtxo: output1, keyPair:randomKeypair}
}

function currencyToUnitI128(currencyAmount: number) {
  let bn = BigNumber.from(currencyAmount);
  return bn.mul(1_000_000_000_000);
}

async function generateVAnchorNote(
  amount: number,
  typedSourceChainId: number,
  typedTargetChainId: number,
  index?: number
) {
  const note = await Note.generateNote({
    amount: String(amount),
    backend: 'Arkworks',
    curve: 'Bn254',
    denomination: String(18),
    exponentiation: String(5),
    hashFunction: 'Poseidon',
    index,
    protocol: 'vanchor',
    sourceChain: String(typedSourceChainId),
    sourceIdentifyingData: '1',
    targetChain: String(typedTargetChainId),
    targetIdentifyingData: '1',
    tokenSymbol: 'WEBB',
    version: 'v1',
    width: String(5),
  });

  return note;
}

function createAccount(accountId: string): any {
  const keyring = new Keyring({ type: 'sr25519' });
  const account = keyring.addFromUri(accountId);

  return account;
}

function createSubstrateResourceId(chainId: number): `0x${string}` {
  let palletIndex = '0x2D';
  let substrateTargetSystem = makeSubstrateTargetSystem(6, palletIndex);
  // set resource ID
  let resourceId = new ResourceId(
    toFixedHex(substrateTargetSystem, 20),
    ChainType.Substrate,
    chainId
  );
  return `0x${resourceId.toString().slice(2)}`;
}
