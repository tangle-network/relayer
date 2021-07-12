export type TestableChain = {
  endpoint: string;
  name: string;
  contractAddress: string;
};

export const getSupportedChain = (chainName: string): TestableChain => {
  const chain = supportedChains.find((entry) => chainName == entry.name);

  if (!chain) {
    throw new Error('Unsupported chain');
  }

  return chain;
};

export const generateRelayerInformationRequest = (chain: TestableChain) => {
  return {
    evm: {
      [chain.name]: {
        information: [],
      },
    },
  };
};

export const generateWithdrawRequest = (
  chain: TestableChain,
  proof: string,
  args: string[]
) => ({
  evm: {
    [chain.name]: {
      relayWithdrew: {
        contract: chain.contractAddress,
        proof,
        root: args[0],
        nullifierHash: args[1],
        recipient: args[2],
        relayer: args[3],
        fee: args[4],
        refund: args[5],
      },
    },
  },
});

const supportedChains: TestableChain[] = [
  {
    endpoint: 'http://127.0.0.1:1998',
    name: 'ganache',
    contractAddress: '',
  },
  {
    endpoint: 'http://beresheet3.edgewa.re:9933',
    name: 'beresheet',
    contractAddress: '0xc0d863EE313636F067dCF89e6ea904AD5f8DEC65',
  },
  {
    endpoint: 'https://api.s1.b.hmny.io',
    name: 'harmony',
    contractAddress: '0x8a4D675dcC71A7387a3C4f27d7D78834369b9542',
  },
];
