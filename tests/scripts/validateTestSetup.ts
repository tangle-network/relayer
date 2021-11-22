import { ethers } from 'ethers';
import { Anchor, Verifier } from 'test-webb-solidity/src/lib/fixed-bridge';
import { GovernedTokenWrapper } from 'test-webb-solidity/src/lib/tokens';
import { PoseidonT3__factory } from 'test-webb-solidity/src/typechain';
import { getAnchorZkComponents } from '../proofUtils';
const snarkjs = require('anchor-snarkjs');
const bigInt = require('big-integer');
const F = require('circomlibjs').babyjub.F;
const Scalar = require("ffjavascript").Scalar;

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

  const zkComponents = await getAnchorZkComponents(1);

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

  let createWitness = async (data: any) => {
    const witnessCalculator = require("../fixtures/2/witness_calculator.js");
    const fileBuf = require('fs').readFileSync('fixtures/2/poseidon_bridge_2.wasm');
    const wtnsCalc = await witnessCalculator(fileBuf)
    const wtns = await wtnsCalc.calculateWTNSBin(data,0);
    return wtns;
  }

  const { pathElements, pathIndices } = await anchor.tree.path(deposit.index);
  const roots = await anchor.populateRootsForProof();

  const input = {
    // public
    nullifierHash: deposit.deposit.nullifierHash,
    refreshCommitment: 0,
    recipient,
    relayer,
    fee: BigInt(0),
    refund: BigInt(0),
    chainID: deposit.deposit.chainID,
    roots: roots,
    // private
    nullifier: deposit.deposit.nullifier,
    secret: deposit.deposit.secret,
    pathElements: pathElements,
    pathIndices: pathIndices,
    diffs: roots.map(r => {
      return F.sub(
        Scalar.fromString(`${r}`),
        Scalar.fromString(`${roots[0]}`),
      ).toString();
    }),
  };

  console.log('input in native: ', input);

  const wtns = await createWitness(input);

  let res = await snarkjs.groth16.prove('./fixtures/2/circuit_final.zkey', wtns);
  const proof = res.proof;
  let publicSignals = res.publicSignals;
  const vKey = await snarkjs.zKey.exportVerificationKey('./fixtures/2/circuit_final.zkey');
  res = await snarkjs.groth16.verify(vKey, publicSignals, proof);
  console.log(res);

  const proofEncoded = await anchor.proveAndVerify(wtns);
  console.log('proof encoded of native input: ', proofEncoded);

  // // construct the withdraw on contract directly
  // const args = [
  //   Anchor.createRootsBytes(input.roots),
  //   toFixedHex(input.nullifierHash),
  //   toFixedHex(input.refreshCommitment, 32),
  //   toFixedHex(input.recipient, 20),
  //   toFixedHex(input.relayer, 20),
  //   toFixedHex(input.fee),
  //   toFixedHex(input.refund),
  // ];
  // const publicInputs = Anchor.convertArgsArrayToStruct(args);

  // const withdrawTx = await anchor.contract.withdraw(`0x${proofEncoded}`, publicInputs, { gasLimit: '0x5B8D80' });
  // await withdrawTx.wait();

  // withdraw the deposit
  // @ts-ignore
  const withdraw = await anchor.withdraw(deposit.deposit, deposit.index, recipient, relayer, BigInt(0), bigInt(0));

  console.log(withdraw);
}

run();