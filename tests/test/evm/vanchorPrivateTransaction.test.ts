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
// This our basic EVM Vanchor Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just relay transactions for us.

import { expect } from 'chai';
import { Tokens, VBridge } from '@webb-tools/protocol-solidity';
import { CircomUtxo, Keypair, parseTypedChainId } from '@webb-tools/sdk-core';
import dotenv from 'dotenv';
import { BigNumber, ethers } from 'ethers';
import temp from 'temp';
import { LocalChain, setupVanchorEvmTx } from '../../lib/localTestnet.js';
import {
  defaultWithdrawConfigValue,
  EnabledContracts,
  EvmFeeInfo,
  EvmEtherscanConfig,
  ResourceMetricResponse,
  WebbRelayer,
  TransactionStatusResponse,
  WithdrawTxSuccessResponse,
  WithdrawTxFailureResponse,
} from '../../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { MintableToken } from '@webb-tools/tokens';
import { formatEther, parseEther } from 'ethers/lib/utils.js';

dotenv.config({ path: '../.env' });

describe('Vanchor Private Tx relaying with mocked governor', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureVBridge: VBridge.VBridge;
  let govWallet1: ethers.Wallet;
  let govWallet2: ethers.Wallet;
  let relayerWallet1: ethers.Wallet;
  let relayerWallet2: ethers.Wallet;

  let webbRelayer: WebbRelayer;

  before(async () => {
    const govPk = u8aToHex(ethers.utils.randomBytes(32));
    const relayerPk = u8aToHex(ethers.utils.randomBytes(32));

    // first we need to start local evm node.
    const localChain1Port = await getPort({
      port: portNumbers(3333, 4444),
    });

    const enabledContracts: EnabledContracts[] = [
      {
        contract: 'VAnchor',
      },
    ];
    parseTypedChainId;
    localChain1 = await LocalChain.init({
      port: localChain1Port,
      chainId: localChain1Port,
      name: 'Hermes',
      populatedAccounts: [
        {
          secretKey: govPk,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
        {
          secretKey: relayerPk,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
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
          secretKey: govPk,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
        {
          secretKey: relayerPk,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
    });

    govWallet1 = new ethers.Wallet(govPk, localChain1.provider());
    govWallet2 = new ethers.Wallet(govPk, localChain2.provider());

    relayerWallet1 = new ethers.Wallet(relayerPk, localChain1.provider());
    relayerWallet2 = new ethers.Wallet(relayerPk, localChain2.provider());
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
      govWallet1
    );
    const unwrappedToken2 = await MintableToken.createToken(
      'Webb Token',
      'WEBB',
      govWallet2
    );

    signatureVBridge = await localChain1.deploySignatureVBridge(
      localChain2,
      wrappedToken1,
      wrappedToken2,
      govWallet1,
      govWallet2,
      unwrappedToken1,
      unwrappedToken2
    );

    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      withdrawConfig: defaultWithdrawConfigValue,
      relayerWallet: relayerWallet1,
      txQueueConfig: { maxSleepInterval: 1500, pollingInterval: 7000 },
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge,
      withdrawConfig: defaultWithdrawConfigValue,
      relayerWallet: relayerWallet2,
      txQueueConfig: { maxSleepInterval: 1500, pollingInterval: 7000 },
    });

    // get the vanhor on localchain1
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(govWallet1);
    // get token
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );

    // aprove token spending for vanchor
    const tx = await token.approveSpending(
      vanchor1.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();

    // mint tokens on wallet
    await token.mintTokens(govWallet1.address, ethers.utils.parseEther('1000'));

    // do the same but on localchain2
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor2.setSigner(govWallet2);

    // Get token
    const tokenAddress2 = signatureVBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      govWallet2
    );

    // Approve token spending for vanchor
    const tx2 = await token2.approveSpending(
      vanchor2.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx2.wait();

    // Mint tokens on wallet
    await token2.mintTokens(
      govWallet2.address,
      ethers.utils.parseEther('1000')
    );

    // Set governor
    const governorAddress = govWallet1.address;
    const currentGovernor = await signatureVBridge
      .getVBridgeSide(localChain1.chainId)
      .contract.governor();
    expect(currentGovernor).to.eq(governorAddress);

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });

    const evmEtherscan: EvmEtherscanConfig = {
      ['mainnet']: {
        chainId: localChain2.underlyingChainId,
        apiKey: process.env.ETHERSCAN_API_KEY!,
      },
    };
    webbRelayer = new WebbRelayer({
      commonConfig: {
        features: { dataQuery: false, governanceRelay: false },
        evmEtherscan,
        port: relayerPort,
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
      verbosity: 3,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should relay private transaction', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(govWallet1);
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor2.setSigner(govWallet2);
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;

    const randomKeypair = new Keypair();

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: ethers.utils.parseEther('1').toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // SignatureVBridge will transact and update the anchors
    await signatureVBridge.transact(
      [],
      [depositUtxo],
      0,
      0,
      '0',
      relayerWallet1.address,
      tokenAddress,
      govWallet1
    );
    // now we wait for the relayer to see the transaction
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    });

    const refundPk = u8aToHex(ethers.utils.randomBytes(32));
    const refundWallet = new ethers.Wallet(refundPk, localChain2.provider());

    const dummyOutput = await setupVanchorEvmTx(
      depositUtxo,
      localChain1,
      localChain2,
      randomKeypair,
      vanchor1,
      vanchor2,
      relayerWallet2,
      tokenAddress,
      0,
      0,
      refundWallet.address
    );

    const gas_amount = await vanchor2.contract.estimateGas.transact(
      dummyOutput.publicInputs.proof,
      '0x0000000000000000000000000000000000000000000000000000000000000000',
      dummyOutput.extData,
      dummyOutput.publicInputs,
      dummyOutput.extData
    );

    const feeInfoResponse = await webbRelayer.getEvmFeeInfo(
      localChain2.underlyingChainId,
      vanchor2.getAddress(),
      gas_amount
    );
    expect(feeInfoResponse.status).equal(200);
    const feeInfo = await (feeInfoResponse.json() as Promise<EvmFeeInfo>);
    console.log(feeInfo);
    const maxRefund = Number(formatEther(feeInfo.maxRefund));
    const refundExchangeRate = Number(formatEther(feeInfo.refundExchangeRate));
    const refundAmount = BigNumber.from(
      parseEther((maxRefund * refundExchangeRate).toString())
    );
    const totalFee = refundAmount.add(feeInfo.estimatedFee);

    const output = await setupVanchorEvmTx(
      depositUtxo,
      localChain1,
      localChain2,
      randomKeypair,
      vanchor1,
      vanchor2,
      relayerWallet2,
      tokenAddress,
      totalFee,
      refundAmount,
      refundWallet.address
    );

    const payload = webbRelayer.vanchorWithdrawPayload(
      localChain2.underlyingChainId,
      vanchor2.getAddress(),
      output.publicInputs,
      output.extData
    );

    const WithdrawTxResponse = await webbRelayer.sendPrivateTxEvm(
      localChain2.underlyingChainId.toString(),
      vanchor2.getAddress(),
      payload
    );

    expect(WithdrawTxResponse.status).equal(200);
    const withdrawTxresp =
      (await WithdrawTxResponse.json()) as WithdrawTxSuccessResponse;
    const itemKey = withdrawTxresp.itemKey;

    // fetch transaction status, it should be in pending state.
    const txStatusResponse = await webbRelayer.getTxStatusEvm(
      localChain2.underlyingChainId.toString(),
      itemKey
    );
    expect(txStatusResponse.status).equal(200);
    const txStatus =
      txStatusResponse.json() as Promise<TransactionStatusResponse>;
    txStatus.then((resp) => {
      expect(resp.status.kind).equal('pending');
    });

    // now we wait for the tx queue on that chain to execute the private transaction.
    await webbRelayer.waitForEvent({
      kind: 'tx_queue',
      event: {
        ty: 'EVM',
        chain_id: localChain2.underlyingChainId.toString(),
        finalized: true,
      },
    });

    // fetch transaction status, it should be in processed state.
    const txStatusResponse2 = await webbRelayer.getTxStatusEvm(
      localChain2.underlyingChainId.toString(),
      itemKey
    );
    expect(txStatusResponse2.status).equal(200);
    const txStatus2 =
      txStatusResponse2.json() as Promise<TransactionStatusResponse>;
    txStatus2.then((resp) => {
      expect(resp.status.kind).equal('processed');
    });

    // fetch resource metrics.
    const response = await webbRelayer.getResourceMetricsEvm(
      localChain2.underlyingChainId.toString(),
      vanchor2.contract.address
    );
    expect(response.status).equal(200);
    const metricsResponse = response.json() as Promise<ResourceMetricResponse>;
    metricsResponse.then((metrics) => {
      console.log(metrics);
      expect(metrics.totalGasSpent).greaterThan(0);
    });
    expect((await refundWallet.getBalance()).eq(refundAmount));
  });

  it.skip('Should fail to withdraw with invalid root', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(govWallet1);

    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor2.setSigner(govWallet2);

    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token

    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(
      govWallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );
    // check webbBalance
    const webbBalance = await token.getBalance(govWallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const randomKeypair = new Keypair();

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // SignatureVBridge will transact and update the anchors
    await signatureVBridge.transact(
      [],
      [depositUtxo],
      0,
      0,
      '0',
      '0',
      tokenAddress,
      govWallet1
    );

    // now we wait for the relayer to see the transaction
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    });

    const output = await setupVanchorEvmTx(
      depositUtxo,
      localChain1,
      localChain2,
      randomKeypair,
      vanchor1,
      vanchor2,
      relayerWallet2,
      tokenAddress,
      0,
      0,
      '0x0000000001000000000100000000010000000001'
    );

    const rootBytes = hexToU8a(output.publicInputs.roots);
    // flip a bit in the proof, so it is invalid
    rootBytes[0] = 0x42;

    const invalidRootBytes = u8aToHex(rootBytes);
    expect(output.publicInputs.roots).to.not.eq(invalidRootBytes);
    output.publicInputs.roots = invalidRootBytes;
    const payload = webbRelayer.vanchorWithdrawPayload(
      localChain2.underlyingChainId,
      vanchor2.getAddress(),
      output.publicInputs,
      output.extData
    );

    const WithdrawTxResponse = await webbRelayer.sendPrivateTxEvm(
      localChain2.underlyingChainId.toString(),
      vanchor2.getAddress(),
      payload
    );

    expect(WithdrawTxResponse.status).equal(200);
    const withdrawTxresp =
      (await WithdrawTxResponse.json()) as WithdrawTxFailureResponse;
    expect(withdrawTxresp.reason).to.contain('Cannot find your merkle root');
  });

  it('Should fail to withdraw with invalid proof', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(govWallet1);

    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor2.setSigner(govWallet2);

    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token

    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(
      govWallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );
    // check webbBalance
    const webbBalance = await token.getBalance(govWallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const randomKeypair = new Keypair();

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // SignatureVBridge will transact and update the anchors
    await signatureVBridge.transact(
      [],
      [depositUtxo],
      0,
      0,
      '0',
      '0',
      tokenAddress,
      govWallet1
    );

    // now we wait for the relayer to see the transaction
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    });

    const output = await setupVanchorEvmTx(
      depositUtxo,
      localChain1,
      localChain2,
      randomKeypair,
      vanchor1,
      vanchor2,
      relayerWallet2,
      tokenAddress,
      0,
      0,
      '0x0000000001000000000100000000010000000001'
    );

    const proofBytes = hexToU8a(output.publicInputs.proof);
    // flip a bit in the proof, so it is invalid
    proofBytes[0] = 0x42;
    const invalidProofBytes = u8aToHex(proofBytes);
    expect(output.publicInputs.proof).to.not.eq(invalidProofBytes);
    output.publicInputs.proof = invalidProofBytes;
    const payload = webbRelayer.vanchorWithdrawPayload(
      localChain2.underlyingChainId,
      vanchor2.getAddress(),
      output.publicInputs,
      output.extData
    );

    const WithdrawTxResponse = await webbRelayer.sendPrivateTxEvm(
      localChain2.underlyingChainId.toString(),
      vanchor2.getAddress(),
      payload
    );

    expect(WithdrawTxResponse.status).equal(200);
    const withdrawTxresp =
      (await WithdrawTxResponse.json()) as WithdrawTxFailureResponse;
    expect(withdrawTxresp.reason).to.contain(
      'Exception while processing transaction'
    );
  });

  it('Should fail to withdraw with invalid nullifier hash', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(govWallet1);

    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor2.setSigner(govWallet2);

    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token

    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(
      govWallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );
    // check webbBalance
    const webbBalance = await token.getBalance(govWallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const randomKeypair = new Keypair();

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // SignatureVBridge will transact and update the anchors
    await signatureVBridge.transact(
      [],
      [depositUtxo],
      0,
      0,
      '0',
      '0',
      tokenAddress,
      govWallet1
    );

    // now we wait for the relayer to see the transaction
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    });

    const output = await setupVanchorEvmTx(
      depositUtxo,
      localChain1,
      localChain2,
      randomKeypair,
      vanchor1,
      vanchor2,
      relayerWallet2,
      tokenAddress,
      0,
      0,
      '0x0000000001000000000100000000010000000001'
    );

    const nullifierHash = hexToU8a(
      output.publicInputs.inputNullifiers[0]?.toHexString()
    );
    // flip a bit in the nullifier, so it is invalid
    nullifierHash[0] = 0x42;

    const invalidnullifierHash = BigNumber.from(u8aToHex(nullifierHash));
    expect(output.publicInputs.inputNullifiers[0]).to.not.eq(
      invalidnullifierHash
    );
    output.publicInputs.inputNullifiers[0] = invalidnullifierHash;
    const payload = webbRelayer.vanchorWithdrawPayload(
      localChain2.underlyingChainId,
      vanchor2.getAddress(),
      output.publicInputs,
      output.extData
    );

    const WithdrawTxResponse = await webbRelayer.sendPrivateTxEvm(
      localChain2.underlyingChainId.toString(),
      vanchor2.getAddress(),
      payload
    );

    expect(WithdrawTxResponse.status).equal(200);
    const withdrawTxresp =
      (await WithdrawTxResponse.json()) as WithdrawTxFailureResponse;
    console.log(withdrawTxresp);
    expect(withdrawTxresp.reason).to.contain(
      'Exception while processing transaction'
    );
  });

  it('should fail to query leaves data api', async () => {
    this.retries(0);
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    // It should fail since data querying is not configured for relayer
    const chainId = localChain1.underlyingChainId.toString();
    const response = await webbRelayer.getLeavesEvm(
      chainId,
      vanchor1.contract.address
    );
    //forbidden access
    expect(response.status).equal(403);
  });

  it('should fail to query encrypted outputs data api', async () => {
    this.retries(0);
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    // It should fail since data querying is not configured for relayer
    const chainId = localChain1.underlyingChainId.toString();
    const response = await webbRelayer.getEncryptedOutputsEvm(
      chainId,
      vanchor1.contract.address
    );
    //forbidden access
    expect(response.status).equal(403);
  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
