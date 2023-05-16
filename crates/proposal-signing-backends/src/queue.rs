use std::sync::atomic::AtomicU32;
use std::sync::{atomic, Arc};

use webb::evm::ethers;
use webb_proposals::{ProposalHeader, ProposalTrait};

pub trait ProposalsQueue {
    type Policy: ProposalsQueuePolicy + Sync + Send + 'static;

    fn enqueue(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
        policy: Self::Policy,
    ) -> webb_relayer_utils::Result<()> {
        policy.check(proposal)?;
        self.enqueue_unchecked(proposal)
    }

    fn dequeue(
        &self,
    ) -> webb_relayer_utils::Result<
        Option<Box<dyn ProposalTrait + Sync + Send + 'static>>,
    >;

    unsafe fn enqueue_unchecked(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> webb_relayer_utils::Result<()>;
}

pub trait ProposalsQueuePolicy {
    fn check(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> webb_relayer_utils::Result<()>;
}

#[impl_trait_for_tuples::impl_for_tuples(5)]
impl ProposalsQueuePolicy for TupleIdentifier {
    fn check(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> webb_relayer_utils::Result<()> {
        for_tuples!( #( TupleIdentifier.check(proposal)?; )* );
        Ok(())
    }
}

/// A smaller version of the proposal, only contains the header and hash.
#[derive(Debug, Clone, Copy)]
struct MiniProposal {
    header: ProposalHeader,
    hash: [u8; 32],
}

impl<P> From<P> for MiniProposal
where
    P: ProposalTrait,
{
    fn from(value: P) -> Self {
        let header = value.header();
        let prop = value.to_vec();
        // only care about the hash of the proposal body.
        let hash = ethers::utils::keccak256(&prop[ProposalHeader::LENGTH..]);
        Self { header, hash }
    }
}

struct AlwaysHigherNoncePolicy {
    last_nonce: Arc<AtomicU32>,
}

impl ProposalsQueuePolicy for AlwaysHigherNoncePolicy {
    fn check(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> webb_relayer_utils::Result<()> {
        let header = proposal.header();
        let nonce = header.nonce().to_u32();
        if nonce <= self.last_nonce.load(atomic::Ordering::SeqCst) {
            Err(webb_relayer_utils::Error::Generic("nonce is too low"))
        } else {
            // update the last nonce
            self.last_nonce.store(nonce, atomic::Ordering::SeqCst);
            Ok(())
        }
    }
}
