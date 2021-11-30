import { ethers } from 'ethers';
import { Anchor, Bridge, BridgeSide } from '@webb-tools/fixed-bridge';
import { MintableToken } from '@webb-tools/tokens';
import { fetchComponentsFromFilePaths, toFixedHex } from '@webb-tools/utils';
import { startGanacheServer } from '../startGanacheServer';
import path from 'path';

const PRIVATE_KEY =
  '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7e';
const ENDPOINT = 'http://127.0.0.1:8545';
const chainIdDest = 8545;

async function runBridge() {
  let chainIdSrc = 3333;
  let providerSrc: ethers.providers.Provider;

  // Ganache setup accounts and servers
  let ganacheAccounts = [
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: "0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7e"
    }
  ];

  let ganacheServer1 = startGanacheServer(
    3333,
    chainIdSrc,
    ganacheAccounts
  );
  providerSrc = new ethers.providers.JsonRpcProvider('http://localhost:3333');
  const walletSrc = new ethers.Wallet(PRIVATE_KEY, providerSrc);

  const provider = new ethers.providers.JsonRpcProvider(ENDPOINT);
  const walletDest = new ethers.Wallet(PRIVATE_KEY, provider);
  const walletAddress = await walletDest.getAddress();

  const tokenInstance1 = await MintableToken.createToken('testToken', 'tTKN', walletSrc);
  await tokenInstance1.mintTokens(walletAddress, '100000000000000000000');
  const tokenInstance2 = await MintableToken.createToken('testToken', 'tTKN', walletDest);

  // Deploy the bridge
  const bridgeInput = {
    anchorInputs: {
      asset: {
        [chainIdSrc]: [tokenInstance1.contract.address],
        [chainIdDest]: [tokenInstance2.contract.address],
      },
      anchorSizes: ['1000000000000000000'],
    },
    chainIDs: [chainIdSrc, chainIdDest],
  };
  const deployerConfig = {
    [chainIdSrc]: walletSrc,
    [chainIdDest]: walletDest
  }
  const zkComponents = await fetchComponentsFromFilePaths(
    path.resolve(__dirname, '../protocol-solidity-fixtures/fixtures/bridge/2/poseidon_bridge_2.wasm'),
    path.resolve(__dirname, '../protocol-solidity-fixtures/fixtures/bridge/2/witness_calculator.js'),
    path.resolve(__dirname, '../protocol-solidity-fixtures/fixtures/bridge/2/circuit_final.zkey')
  );
  
  const bridge = await Bridge.deployBridge(bridgeInput, deployerConfig, zkComponents);

  console.log('finished bridge deployments');

  const recipient = "0x1111111111111111111111111111111111111111";
  const relayer = "0x2222222222222222222222222222222222222222";

  const srcAnchor = await bridge.getAnchor(chainIdSrc, '1000000000000000000');
  await srcAnchor.setSigner(walletSrc);

  // approve token spending
  const webbToken1Address = await bridge.getWebbTokenAddress(chainIdSrc)!;
  const webbToken1 = await MintableToken.tokenFromAddress(webbToken1Address, walletSrc);
  await webbToken1.mintTokens(walletAddress, '1000000000000000000000');
  await webbToken1.approveSpending(srcAnchor.contract.address);

  console.log('token spending approved');

  // deposit
  const deposit = await srcAnchor.deposit(chainIdDest);

  console.log('after deposit');

  // generate the merkle proof from the source anchor
  const { pathElements, pathIndices } = srcAnchor.tree.path(deposit.index);

  // update the destAnchor
  let destAnchor = await bridge.getAnchor(chainIdDest, '1000000000000000000');

  // BridgeSide interacted with externally 
  const bridgeSide2: BridgeSide = bridge.getBridgeSide(chainIdDest);
  await bridgeSide2.voteProposal(srcAnchor, destAnchor);
  await bridgeSide2.executeProposal(srcAnchor, destAnchor);

  const destRoots = await destAnchor.populateRootsForProof();
  console.log(`destRoots: ${destRoots}`);
  console.log(`destAnchor tree root: ${destAnchor.tree.get_root()}`);

  const input = await destAnchor.generateWitnessInput(
    deposit.deposit,
    deposit.originChainId,
    0,
    BigInt(recipient),
    BigInt(relayer),
    BigInt(0),
    BigInt(0),
    destRoots,
    pathElements,
    pathIndices
  );
  console.log('after generate witness input');
  const wtns = await destAnchor.createWitness(input);
  const proofEncoded = await destAnchor.proveAndVerify(wtns);
  const proofArgs = [
    Anchor.createRootsBytes(input.roots),
    toFixedHex(input.nullifierHash),
    toFixedHex(input.refreshCommitment, 32),
    toFixedHex(input.recipient, 20),
    toFixedHex(input.relayer, 20),
    toFixedHex(input.fee),
    toFixedHex(input.refund),
  ]

  const proof = `0x${proofEncoded}`;
  const args = Anchor.convertArgsArrayToStruct(proofArgs);

  const tx = await destAnchor.contract.withdraw(proof, args);
  const receipt = await tx.wait();
  console.log(receipt);
  ganacheServer1.close(console.error);
}

runBridge();
// run();