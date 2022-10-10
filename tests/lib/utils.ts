import { Keypair, Utxo } from '@webb-tools/sdk-core';
import { EventsWatcher } from './webbRelayer';

// Default Events watcher for the pallets.
export const defaultEventsWatcherValue: EventsWatcher = {
  enabled: true,
  pollingInterval: 3000,
};

export async function generateArkworksUtxoTest(
  amount: number,
  chainId: number,
  outputChainId: number,
  index?: number,
  keypair?: Keypair,
) {

  const utxo = await Utxo.generateUtxo({
    amount: String(amount),
    backend: 'Arkworks',
    chainId: String(chainId),
    originChainId: String(outputChainId),
    curve: 'Bn254',
    index: index ? String(index) : undefined,
    keypair: keypair ?? new Keypair(),
  })

  return utxo;
}
