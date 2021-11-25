import { ethers } from 'ethers';
import { Verifier, Anchor } from '@nepoche/fixed-bridge';
import { GovernedTokenWrapper } from '@nepoche/tokens';
import { PoseidonT3__factory } from '@nepoche/contracts';
import { fetchComponentsFromFilePaths } from '@nepoche/utils';
// import { getAnchorZkComponents } from '../proofUtils';
import path from 'path';
// const snarkjs = require('anchor-snarkjs');
const bigInt = require('big-integer');
// const F = require('circomlibjs').babyjub.F;
// const Scalar = require("ffjavascript").Scalar;

const PRIVATE_KEY =
  '0xc0d375903fd6f6ad3edafc2c5428900c0757ce1da10e5dd864fe387b32b91d7e';
const ENDPOINT = 'http://127.0.0.1:8545';
async function run() {
  const provider = new ethers.providers.JsonRpcProvider(ENDPOINT);
  const wallet = new ethers.Wallet(PRIVATE_KEY, provider);
  const walletAddress = await wallet.getAddress();
  const recipient = "0x1111111111111111111111111111111111111111";
  const relayer = "0x2222222222222222222222222222222222222222";

  // create poseidon hasher
  const hasherFactory = new PoseidonT3__factory(wallet);
  const hasherInstance = await hasherFactory.deploy();
  await hasherInstance.deployed();

  const verifier = await Verifier.createVerifier(wallet);
  let verifierInstance = verifier.contract;

  const zkComponents = await fetchComponentsFromFilePaths(
    path.resolve(__dirname, '../fixtures/2/poseidon_bridge_2.wasm'),
    path.resolve(__dirname, '../fixtures/2/witness_calculator.js'),
    path.resolve(__dirname, '../fixtures/2/circuit_final.zkey')
  );

  const tokenWrapper = await GovernedTokenWrapper.createGovernedTokenWrapper(
    'testToken',
    'tTKN',
    walletAddress,
    '10000000000000000000000000',
    true,
    wallet
  );

  const anchor = await Anchor.createAnchor(
    verifierInstance.address,
    hasherInstance.address,
    '1000000000000000000',
    30,
    tokenWrapper.contract.address,
    walletAddress,
    walletAddress,
    walletAddress,
    1,
    zkComponents,
    wallet
  );

  await tokenWrapper.grantMinterRole(anchor.contract.address);
  await tokenWrapper.contract.mint(walletAddress, '10000000000000000000000');
  await tokenWrapper.contract.approve(anchor.contract.address, '10000000000000000000000');

  // create a deposit
  const deposit = await anchor.deposit();

  const withdrawSetup = await anchor.setupWithdraw(deposit.deposit, deposit.index, recipient, relayer, BigInt(0), bigInt(0));

  const proof = `0x${withdrawSetup.proofEncoded}`;
  const args = withdrawSetup.args;
  const publicInputs = Anchor.convertArgsArrayToStruct(args);

  const withdraw = await anchor.contract.withdraw(proof, publicInputs, { gasLimit: "0x5B8D80" });

  // withdraw the deposit
  // const withdraw = await anchor.withdraw(deposit.deposit, deposit.index, recipient, relayer, BigInt(0), bigInt(0));

  console.log(withdraw);
}

run();