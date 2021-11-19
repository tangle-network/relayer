import { ethers } from 'ethers';
import WebSocket from 'ws';
import {
  generateSnarkProof,
  getTornadoDenomination,
  getDepositLeavesFromRelayer,
  calculateFee,
  deposit,
} from '../proofUtils';
import {
  getRelayerConfig,
  generateAnchorWithdrawRequest,
  startWebbRelayer,
  handleMessage,
  sleep,
  Result,
} from '../relayerUtils';

type sanityChainConfig = {
  name: string;
  endpoint: string;
  contractAddress: string;
  private_key: string;
};

let chains: sanityChainConfig[] = [
  {
    name: 'beresheet',
    endpoint: 'http://beresheet3.edgewa.re:9933',
    contractAddress: '0xc0d863EE313636F067dCF89e6ea904AD5f8DEC65',
    private_key:
      '1749563947452850678456352849674537239203756595873523849581626549',
  },
  {
    name: 'harmony',
    endpoint: 'https://api.s1.b.hmny.io',
    contractAddress: '0x4c37863bf2642Ba4e8De7e746500C700540119E8',
    private_key:
      '1749563947452850678456352849674537239203756595873523849581626549',
  },
  {
    name: 'rinkeby',
    endpoint: 'https://rinkeby.infura.io/v3/9aa3d95b3bc440fa88ea12eaa4456161',
    contractAddress: '0x626FEc5Ffa7Bf1EE8CEd7daBdE545630473E3ABb',
    private_key:
      '1749563947452850678456352849674537239203756595873523849581626549',
  },
];

let provider, wallet;
const relayer = startWebbRelayer();

async function main() {
  await sleep(4000); // just to wait for the relayer start-up

  for (let i = 0; i < chains.length; i++) {
    provider = new ethers.providers.JsonRpcProvider(chains[i]!.endpoint);
    wallet = new ethers.Wallet(chains[i]!.private_key, provider);

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
    const depositArgs = await deposit(chains[i]!.contractAddress, wallet);
    console.log('Deposit Done ..');
    console.log('Starting the Relayer ..');

    // get all relayer information
    const relayerInfo = await getRelayerConfig(
      'beresheet',
      'http://localhost:9955'
    );
    const contractAddress = chains[i]!.contractAddress;
    const contractDenomination = await getTornadoDenomination(
      contractAddress,
      provider
    );
    const calculatedFee = calculateFee(
      relayerInfo.contracts[contractAddress].withdrawFeePercentage,
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
        client.terminate();
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
        client.close();
      } else {
        // ??
      }
    });
    client.on('error', (err) => {
      console.log('[E]', err);
      client.terminate();
    });
    console.log('Generating zkProof to do a withdraw ..');
    // Since this sanity test spins up its own local relayer, fetch the leaves from a well-known relayer here
    console.log('Allow time for the leaf-cache relayer to see the new leaf');
    await sleep(30000); // this should match polling interval of connected relayer
    const leaves = await getDepositLeavesFromRelayer(
      chains[i]!.contractAddress,
      `${process.env.RELAYER_ENDPOINT_HTTP}`
    );
    const { proof, args } = await generateSnarkProof(
      leaves,
      depositArgs,
      recipient,
      relayerInfo.beneficiary,
      calculatedFee
    );
    console.log('Proof Generated!');
    const req = generateAnchorWithdrawRequest(
      chains[i]!.name,
      chains[i]!.contractAddress,
      proof,
      args
    );
    if (client.readyState === client.OPEN) {
      const data = JSON.stringify(req);
      console.log('Sending Proof to the Relayer ..');
      console.log('=>', data);
      client.send(data, (err) => {
        console.log('Proof Sent!');
        if (err !== undefined) {
          console.log('!!Error!!', err);
          client.terminate();
        }
      });
      console.log('Waiting for the relayer to finish the Tx ..');
      await sleep(20_000);
    } else {
      client.terminate();
      console.error('Relayer Connection closed!');
    }
  }

  relayer.kill();
  process.exit(1);
}

main().catch(console.error);
