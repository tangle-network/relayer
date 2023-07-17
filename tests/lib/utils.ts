import { Keyring } from '@polkadot/api';
import { KeyringPair } from '@polkadot/keyring/types';
import { EventsWatcher } from './webbRelayer';

// Default Events watcher for the pallets.
export const defaultEventsWatcherValue: EventsWatcher = {
  enabled: true,
  pollingInterval: 3000,
  printProgressInterval: 60_000,
  syncBlocksFrom: 0,
};

// Pad hexString with zero to make it of length 64.
export function padHexString(hexString: string): string {
  return '0x' + hexString.substring(2).padStart(64, '0');
}

export function createAccount(accountId: string): KeyringPair {
  const keyring = new Keyring({ type: 'sr25519' });
  const account = keyring.addFromUri(accountId);
  return account;
}
