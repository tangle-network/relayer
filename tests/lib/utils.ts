import { Keypair, Note, Utxo } from '@webb-tools/sdk-core';
import { EventsWatcher } from './webbRelayer';

// Default Events watcher for the pallets.
export const defaultEventsWatcherValue: EventsWatcher = {
  enabled: true,
  pollingInterval: 3000,
};

// Pad hexString with zero to make it of length 64.
export function padHexString(hexString: string): string {
  return '0x' + hexString.substring(2).padStart(64, '0');
}

export async function generateArkworksUtxoTest(
  amount: number,
  chainId: number,
  outputChainId: number,
  index?: number,
  keypair?: Keypair
) {
  const utxo = await Utxo.generateUtxo({
    amount: String(amount),
    backend: 'Arkworks',
    chainId: String(chainId),
    originChainId: String(outputChainId),
    curve: 'Bn254',
    index: index ? String(index) : undefined,
    keypair: keypair ?? new Keypair(),
  });

  return utxo;
}
// utility function to create vanchor note.
export async function generateVAnchorNote(
  amount: number,
  typedSourceChainId: number,
  typedTargetChainId: number,
  index?: number
) {
  const note = await Note.generateNote({
    amount: String(amount),
    backend: 'Arkworks',
    curve: 'Bn254',
    denomination: String(18),
    exponentiation: String(5),
    hashFunction: 'Poseidon',
    index,
    protocol: 'vanchor',
    sourceChain: String(typedSourceChainId),
    sourceIdentifyingData: '1',
    targetChain: String(typedTargetChainId),
    targetIdentifyingData: '1',
    tokenSymbol: 'WEBB',
    version: 'v1',
    width: String(5),
  });

  return note;
}
