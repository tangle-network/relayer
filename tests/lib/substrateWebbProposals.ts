import { u8aToHex, hexToU8a } from '@polkadot/util';
import { ProposalHeader } from '@webb-tools/sdk-core/proposals';

export interface SubstrateResourceIdUpdateProposal {
  /**
   * The ResourceIdUpdateProposal Header.
   * This is the first 40 bytes of the proposal.
   * See `encodeProposalHeader` for more details.
   */
  readonly header: ProposalHeader;
  /**
   * 32 bytes Hex-encoded string.
   */
  readonly newResourceId: string;
  /**
   * 1 byte Hex-encoded string.
   */
  readonly palletIndex: string;
  /**
   * 1 byte Hex-encoded string.
   */
  readonly callIndex: string;
}
export function encodeResourceIdUpdateProposal(
  proposal: SubstrateResourceIdUpdateProposal
): Uint8Array {
  const resourceIdUpdateProposal = new Uint8Array(74);
  resourceIdUpdateProposal.set(proposal.header.toU8a(), 0); // 0 -> 40
  const palletIndex = hexToU8a(proposal.palletIndex).slice(0, 1);
  resourceIdUpdateProposal.set(palletIndex, 40); // 40 -> 41
  const callIndex = hexToU8a(proposal.callIndex).slice(0, 1);
  resourceIdUpdateProposal.set(callIndex, 41); // 40 -> 41
  const newResourceId = hexToU8a(proposal.newResourceId).slice(0, 32);
  resourceIdUpdateProposal.set(newResourceId, 42); // 42 -> 74
  return resourceIdUpdateProposal;
}

export function decodeResourceIdUpdateProposal(
  data: Uint8Array
): SubstrateResourceIdUpdateProposal {
  const header = ProposalHeader.fromBytes(data.slice(0, 40));
  const palletIndex = u8aToHex(data.slice(40, 41)); // 40 -> 41
  const callIndex = u8aToHex(data.slice(41, 42)); // 41 -> 42
  const newResourceId = u8aToHex(data.slice(42, 74)); // 42 -> 74

  return {
    header,
    newResourceId,
    palletIndex,
    callIndex,
  };
}
