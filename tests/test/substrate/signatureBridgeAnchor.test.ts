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
// This our basic Substrate Anchor Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just to relay transactions for us.

import '@webb-tools/types';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import isCi from 'is-ci';
import { ethers } from 'ethers';
import { WebbRelayer, Pallet, getChainIdType,toFixedHex, convertToHexNumber, toHex } from '../../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../../lib/localProtocolSubstrate.js';
import { hexToU8a, u8aToHex } from '@polkadot/util';
import {
  UsageMode,
  defaultEventsWatcherValue,
} from '../../lib/substrateNodeBase.js';
import { ApiPromise, Keyring } from '@polkadot/api';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { BigNumber } from 'ethers';
import { Note, NoteGenInput } from '@webb-tools/sdk-core';
import {
  encodeResourceIdUpdateProposal,
  ResourceIdUpdateProposal,
} from '../../lib/substrateWebbProposals.js';
import { ChainIdType, makeResourceId } from '../../lib/webbProposals.js';
import pkg from 'secp256k1';
const { ecdsaSign } = pkg;
describe('Substrate Signature Bridge Relaying On Anchor Deposit <> Mocked Backend', function () {
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;

  let webbRelayer: WebbRelayer;

  // Governer key
  const PK1 = u8aToHex(ethers.utils.randomBytes(32));
  let governorWallet = new ethers.Wallet(PK1)
  // slice 0x04 from public key
  let uncompressedKey = governorWallet._signingKey().publicKey.slice(4);

  let typedSourceChainId = getChainIdType(ChainIdType.SUBSTRATE, 1080);
  
  before(async () => {
    // now we start webb-protocol on substrate
    const usageMode: UsageMode = isCi
      ? { mode: 'docker', forcePullImage: false }
      : {
          mode: 'host',
          nodePath: path.resolve(
            '../../protocol-substrate/target/release/webb-standalone-node'
          ),
        };

    // enable pallets
    const enabledPallets: Pallet[] = [
      {
        pallet: 'AnchorBn254',
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
    });

    // Wait until we are ready and connected
    const api = await aliceNode.api();
    await api.isReady;

    let chainId = await aliceNode.getChainId();

    // set proposal signing backend and linked anchors
    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      chainId: chainId,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
      linkedAnchors: [
        {
          chain: 1080,
          tree: 9,
        },
      ],
    });

    //force set maintainer
    let setMaintainerCall = api.tx.signatureBridge!.forceSetMaintainer!(
      Array.from(hexToU8a(uncompressedKey))
    );
    await aliceNode.sudoExecuteTransaction(setMaintainerCall);

     //whitelist chain
     let whitelistChainCall2 = api.tx.signatureBridge!.whitelistChain!(typedSourceChainId);
     await aliceNode.sudoExecuteTransaction(whitelistChainCall2);

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(8000, 8888) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
      showLogs: false,
    });
    await webbRelayer.waitUntilReady();
  });

  it('Relayer should create and relay anchor update proposal to signature bridge for execution', async () => {
    const api = await aliceNode.api();
    // create anchor
    const treeId = await createAnchor(api, aliceNode);
    const account = createAccount('//Dave');

    // chainId
    let chainId = await aliceNode.getChainId();

    // now we set resource through proposal execution
    let setResourceIdProposalCall = await setResourceIdProposal(api,PK1,treeId,chainId);
    const txSigned = await setResourceIdProposalCall.signAsync(account);
    await aliceNode.executeTransaction(txSigned);
    
    const note = await makeDeposit(api, aliceNode, treeId);

    // now we wait for the proposal to be signed by mocked backend and then send data to signature bridge
    await webbRelayer.waitForEvent({
      kind: 'signing_backend',
      event: {
        backend: 'Mocked',
      },
    });

    // now we wait for the proposals to verified and executed by signature bridge

    await webbRelayer.waitForEvent({
      kind: 'signature_bridge',
      event: {
        call: 'execute_proposal_with_signature',
        finalized: true,
      },
    });
  });

  after(async () => {
    await aliceNode?.stop();
    await bobNode?.stop();
    await webbRelayer?.stop();
  });
});

async function createAnchor(api: ApiPromise, aliceNode: LocalProtocolSubstrate) : Promise<number> {
  let createAnchorCall = api.tx.anchorBn254.create(10000000000000, 100, 32,0);
     // execute sudo transaction.
    await aliceNode.sudoExecuteTransaction(createAnchorCall);
    // get latest treeId created for anchor
    const treeIds = await api.query.anchorBn254.anchors.keys();
    const sorted = treeIds.map((id) => Number(id.toHuman())).sort().reverse();
    const treeId = sorted[0] || 9;
    return treeId
}

async function setResourceIdProposal(api:ApiPromise, PK1: string, treeId:number, chainId: number): Promise<SubmittableExtrinsic<'promise'>>{
  // set resource ID
  let resourceId = makeResourceId(toHex(treeId, 20),ChainIdType.SUBSTRATE,chainId);
  let functionSignature = toFixedHex(0,4);
  let nonce = BigNumber.from(1);
  let newResourceId = resourceId;
  let targetSystem = '0x0109000000';
  let palletIndex = convertToHexNumber(44);
  let callIndex = convertToHexNumber(2);
  
  const resourceIdUpdateProposalPayload: ResourceIdUpdateProposal = {
    header: {
      resourceId,
      functionSignature: functionSignature,
      nonce: nonce.toNumber(),
      chainIdType: ChainIdType.SUBSTRATE,
      chainId: chainId,
    },
    newResourceId,
    targetSystem,
    palletIndex,
    callIndex,
  };
  // encode resourse update proposal
  let proposalBytes = encodeResourceIdUpdateProposal(
    resourceIdUpdateProposalPayload
  );
  let hash = ethers.utils.keccak256(proposalBytes);
  let msg = ethers.utils.arrayify(hash);

  // sign the message
  const sigObj = ecdsaSign(msg, hexToU8a(PK1));
  let signature = new Uint8Array([...sigObj.signature, sigObj.recid])
  // execute proposal call to handler
  let executeSetProposalCall = api.tx.anchorHandlerBn254!.executeSetResourceProposal!(resourceId,targetSystem);
  let setResourceCall = api.tx.signatureBridge!.setResourceWithSignature!(
   getChainIdType(ChainIdType.SUBSTRATE, chainId),
   executeSetProposalCall,
   u8aToHex(proposalBytes),
   u8aToHex(signature)
  );
  return setResourceCall
}

async function createAnchorDepositTx(api: ApiPromise, treeId: number): Promise<{
  tx: SubmittableExtrinsic<'promise'>;
  note: Note;
}> {
  const noteInput: NoteGenInput = {
    protocol: 'anchor',
    version: 'v2',
    sourceChain: '2199023256632',
    targetChain: '2199023256632',
    sourceIdentifyingData: treeId.toString(),
    targetIdentifyingData: treeId.toString(),
    tokenSymbol: 'WEBB',
    amount: '100',
    denomination: '18',
    backend: 'Arkworks',
    hashFunction: 'Poseidon',
    curve: 'Bn254',
    width: '4',
    exponentiation: '5',
  };
  const note = await Note.generateNote(noteInput);
  const leaf = note.getLeaf();
  const tx = api.tx.anchorBn254.deposit(treeId, leaf);
  return { tx, note };
}

function createAccount(accountId: string): any {
  const keyring = new Keyring({ type: 'sr25519' });
  const account = keyring.addFromUri(accountId);

  return account;
}

async function makeDeposit(
  api: ApiPromise,
  aliceNode: LocalProtocolSubstrate,
  treeId: number,
): Promise<Note> {
  const { tx, note } = await createAnchorDepositTx(api,treeId);
  const account = createAccount('//Dave');
  // send the deposit transaction.
  const txSigned = await tx.signAsync(account);

  await aliceNode.executeTransaction(txSigned);

  return note;
}