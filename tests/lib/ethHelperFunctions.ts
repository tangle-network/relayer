import { ethers } from 'ethers';

export function ethAddressFromUncompressedPublicKey(
  publicKey: `0x${string}`
): `0x${string}` {
  const pubKeyHash = ethers.utils.keccak256(publicKey); // we hash it.
  const address = ethers.utils.getAddress(`0x${pubKeyHash.slice(-40)}`); // take the last 20 bytes and convert it to an address.
  return address as `0x${string}`;
}
