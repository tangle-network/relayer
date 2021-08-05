import { ethers } from 'ethers';
import WebSocket from 'ws';
import { 
  generateSnarkProof,
  getAnchorDenomination,
  getDepositLeavesFromServer,
  calculateFee,
  deposit
} from '../proofUtils';
import { 
  getRelayerConfig,
  generateWithdrawRequest,
  startWebbRelayer,
  handleMessage,
  sleep,
  Result
} from '../relayerUtils';

const ENDPOINT = 'http://beresheet3.edgewa.re:9933';
const contractAddress = '0xc0d863EE313636F067dCF89e6ea904AD5f8DEC65';
const PRIVATE_KEY = '1749563947452850678456352849674537239203756595873523849581626549';

let provider, wallet;

async function main() {
  provider = new ethers.providers.JsonRpcProvider(ENDPOINT);
  wallet = new ethers.Wallet(PRIVATE_KEY, provider);

  const recipient = ethers.utils.getAddress(
    '0xe8f999AC5DAa08e134735016FAcE0D6439baFF94'
  );
  let startingRecipientBalance = await provider.getBalance(recipient);
  console.log(
    `Starting balance of recipient equal to ${ethers.utils.formatEther(
      startingRecipientBalance
    )} UNIT`
  );

  console.log('Sending Deposit Tx to the contract ..');
  const depositArgs = await deposit(contractAddress, wallet);
  await sleep(1500); // allow time for the server to update leaves
  console.log('Deposit Done ..');
  console.log('Starting the Relayer ..');
  const relayer = await startWebbRelayer();
  await sleep(4000); // just to wait for the relayer start-up

  // get all relayer information
  const relayerInfo = await getRelayerConfig('beresheet');
  const contractDenomination = await getAnchorDenomination(
    contractAddress,
    provider
  );
  const calculatedFee = calculateFee(
    relayerInfo.withdrawFeePercentage,
    contractDenomination
  );

  const client = new WebSocket('ws://localhost:9955/ws');
  await new Promise((resolve) => client.on('open', resolve));
  console.log('Connected to Relayer!');

  client.on('message', async (data) => {
    console.log('<==', data);
    const msg = JSON.parse(data as string);
    const result = await handleMessage(msg);
    if (result === Result.Errored) {
      relayer.kill('SIGTERM');
      client.terminate();
      process.exit(1);
    } else if (result === Result.Continue) {
      // all good.
      return;
    } else if (result === Result.CleanExit) {
      console.log('Transaction Done and Relayed Successfully!');
      console.log(`Checking balance of the recipient (${recipient})`);
      // check the recipient balance
      const endingRecipientBalance = await provider.getBalance(recipient);
      console.log(
        `Balance equal to ${ethers.utils.formatEther(
          endingRecipientBalance
        )} UNIT`
      );
      console.log('Clean Exit');
      relayer.kill('SIGINT');
      client.close();
      process.exit(0);
    } else {
      // ??
    }
  });
  client.on('error', (err) => {
    console.log('[E]', err);
    relayer.kill('SIGTERM');
    client.terminate();
    process.exit(1);
  });
  console.log('Generating zkProof to do a withdraw ..');
  const leaves = await getDepositLeavesFromServer(contractAddress);
  const { proof, args } = await generateSnarkProof(
    leaves,
    depositArgs,
    recipient,
    relayerInfo.account,
    calculatedFee
  );
  console.log('Proof Generated!');
  const req = generateWithdrawRequest("beresheet", contractAddress, proof, args);
  if (client.readyState === client.OPEN) {
    const data = JSON.stringify(req);
    console.log('Sending Proof to the Relayer ..');
    console.log('=>', data);
    client.send(data, (err) => {
      console.log('Proof Sent!');
      if (err !== undefined) {
        console.log('!!Error!!', err);
        relayer.kill('SIGTERM');
        client.terminate();
        process.exit(1);
      }
    });
    console.log('Waiting for the relayer to finish the Tx ..');
    await sleep(45_000);
  } else {
    client.terminate();
    console.error('Relayer Connection closed!');
    relayer.kill();
    process.exit(1);
  }

  relayer.kill('SIGINT');
  process.exit();
}

main().catch(console.error);
