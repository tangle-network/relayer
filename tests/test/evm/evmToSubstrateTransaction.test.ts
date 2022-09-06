// /*
//  * Copyright 2022 Webb Technologies Inc.
//  *
//  * Licensed under the Apache License, Version 2.0 (the "License");
//  * you may not use this file except in compliance with the License.
//  * You may obtain a copy of the License at
//  *
//  * http://www.apache.org/licenses/LICENSE-2.0
//  *
//  * Unless required by applicable law or agreed to in writing, software
//  * distributed under the License is distributed on an "AS IS" BASIS,
//  * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  * See the License for the specific language governing permissions and
//  * limitations under the License.
//  *
//  */
// // This our basic EVM Vanchor Transaction Relayer Tests.
// // These are for testing the basic relayer functionality. which is just relay transactions for us.

// /**
//  * Deploy VBridge with single Chain
//  * Run Substrate node 
//  * create new vanchor for substrate
//  * Deposit 
//  */


// import { expect } from 'chai';
// import { Tokens, VBridge } from '@webb-tools/protocol-solidity';
// import { CircomUtxo } from '@webb-tools/sdk-core';
// import { ethers } from 'ethers';
// import path from 'path';
// import temp from 'temp';
// import { LocalChain } from '../../lib/localTestnet.js';
// import {
//   EnabledContracts,
//   LeavesCacheResponse,
//   WebbRelayer,
// } from '../../lib/webbRelayer.js';
// import getPort, { portNumbers } from 'get-port';
// import { u8aToHex } from '@polkadot/util';
// import { LocalProtocolSubstrate } from 'lib/localProtocolSubstrate.js';
// import { UsageMode } from 'lib/substrateNodeBase.js';
// import isCi from 'is-ci';
// // const assert = require('assert');
// describe('Evm Substrate cross transaction', function () {
//   const tmpDirPath = temp.mkdirSync();
//   let localChain1: LocalChain;
//   let localChain2: LocalProtocolSubstrate;
//   let signatureVBridge: VBridge.VBridge;
//   let wallet1: ethers.Wallet;
//   let wallet2: ethers.Wallet;

//   let webbRelayer: WebbRelayer;

//   before(async () => {
//     const PK1 = u8aToHex(ethers.utils.randomBytes(32));
//     // first we need to start local evm node.
//     const localChain1Port = await getPort({
//       port: portNumbers(3333, 4444),
//     });

//     const enabledContracts: EnabledContracts[] = [
//       {
//         contract: 'VAnchor',
//       },
//     ];
//     localChain1 = new LocalChain({
//       port: localChain1Port,
//       chainId: 31337,
//       name: 'Hermes',
//       populatedAccounts: [
//         {
//           secretKey: PK1,
//           balance: ethers.utils
//             .parseEther('100000000000000000000000')
//             .toHexString(),
//         },
//       ],
//       enabledContracts: enabledContracts,
//     });

//     const usageMode: UsageMode = isCi
//       ? { mode: 'docker', forcePullImage: false }
//       : {
//           mode: 'host',
//           nodePath: path.resolve(
//             '../../../protocol-substrate/target/release/webb-standalone-node'
//           ),
//         };

//     localChain2 = await LocalProtocolSubstrate.start({
//       name: 'substrate-alice',
//       authority: 'alice',
//       usageMode,
//       ports: 'auto',
//     });

//     // Wait until we are ready and connected
//     const api = await localChain2.api();
//     await api.isReady;

//     wallet1 = new ethers.Wallet(PK1, localChain1.provider());

//     // Deploy the token.
//     const localToken1 = await localChain1.deployToken(
//       'Webb Token',
//       'WEBB',
//       wallet1
//     );

//     // save the chain configs.
//     await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
//       signatureVBridge,
//       proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
//     });

//     let chainI2 = await localChain2.getChainId();
//     // save substrate chain config
//     await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
//         suri: '//Charlie',
//         chainId: chainI2,
//     });

//     const GOV = u8aToHex(ethers.utils.randomBytes(32));
//     const govWallet = new ethers.Wallet(GOV);
//     signatureVBridge = await localChain1.deployVBridge(
//       localToken1,
//       wallet1,
//       {
//         [localChain1.chainId]: govWallet,
//       }
//     );

//     // get the vanhor on localchain1
//     const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
//     await vanchor1.setSigner(wallet1);
//     // approve token spending
//     const tokenAddress = signatureVBridge.getWebbTokenAddress(
//       localChain1.chainId
//     )!;
//     const token = await Tokens.MintableToken.tokenFromAddress(
//       tokenAddress,
//       wallet1
//     );
//     await token.approveSpending(vanchor1.contract.address);
//     await token.mintTokens(
//       wallet1.address,
//       ethers.utils.parseEther('100000000000000000000000')
//     );

//     // now start the relayer
//     const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
//     webbRelayer = new WebbRelayer({
//       port: relayerPort,
//       tmp: true,
//       configDir: tmpDirPath,
//       showLogs: false,
//     });
//     await webbRelayer.waitUntilReady();
//   });

//   it('number of deposits made should be equal to number of leaves in cache', async () => {
//      // get the vanhor on localchain1
//      const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
//      await vanchor1.setSigner(wallet1);
//   });

//   after(async () => {
//     await localChain1?.stop();
//     await localChain2?.stop();
//     await webbRelayer?.stop();
//   });
// });
