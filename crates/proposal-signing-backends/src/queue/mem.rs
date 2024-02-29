// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;

use super::{policy::*, ProposalHash, QueuedAnchorUpdateProposal};

/// In memory implementation of the proposals queue.
#[derive(Clone, Debug, Default)]
pub struct InMemoryProposalsQueue {
    proposals: Arc<RwLock<VecDeque<QueuedAnchorUpdateProposal>>>,
}

impl InMemoryProposalsQueue {
    /// Creates a new `InMemoryProposalsQueue` that will store the proposals
    /// in memory.
    pub fn new() -> Self {
        Self {
            proposals: Arc::new(RwLock::new(Default::default())),
        }
    }
}

impl super::ProposalsQueue for InMemoryProposalsQueue {
    type Proposal = QueuedAnchorUpdateProposal;

    #[tracing::instrument(
        skip_all,
        fields(
            queue = "in_memory",
            proposal = hex::encode(proposal.full_hash()),
        )
    )]
    fn enqueue<Policy: ProposalPolicy>(
        &self,
        proposal: Self::Proposal,
        policy: Policy,
    ) -> webb_relayer_utils::Result<()> {
        let accepted = policy.check(&proposal, self);
        tracing::trace!(accepted = ?accepted, "proposal check result");
        accepted?;
        self.proposals.write().push_back(proposal);
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(queue = "in_memory"))]
    fn dequeue<Policy: ProposalPolicy>(
        &self,
        policy: Policy,
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>> {
        if self.proposals.read().is_empty() {
            tracing::trace!("no proposals to dequeue");
            return Ok(None);
        }
        let proposal = match self.proposals.write().pop_front() {
            Some(proposal) => proposal,
            None => return Ok(None),
        };
        match policy.check(&proposal, self) {
            Ok(_) => {
                tracing::trace!(
                    proposal = hex::encode(proposal.full_hash()),
                    "proposal passed policy check before dequeue",
                );
                Ok(Some(proposal))
            }
            Err(e) => {
                tracing::trace!(
                    reason = %e,
                    "proposal failed policy check before dequeue",
                );
                // push back the proposal if it failed the policy check
                // so that it can be dequeued again later
                self.proposals.write().push_back(proposal);
                // the caller should try again later
                Ok(None)
            }
        }
    }

    fn len(&self) -> webb_relayer_utils::Result<usize> {
        let len = self.proposals.read().len();
        Ok(len)
    }

    fn is_empty(&self) -> webb_relayer_utils::Result<bool> {
        Ok(self.proposals.read().is_empty())
    }

    fn clear(&self) -> webb_relayer_utils::Result<()> {
        self.proposals.write().clear();
        Ok(())
    }

    fn retain<F>(&self, f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&Self::Proposal) -> bool,
    {
        self.proposals.write().retain(f);
        Ok(())
    }

    fn modify_in_place<F>(&self, f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&mut Self::Proposal) -> webb_relayer_utils::Result<()>,
    {
        self.proposals.write().iter_mut().try_for_each(f)
    }

    fn find<F>(
        &self,
        mut f: F,
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>>
    where
        F: FnMut(&Self::Proposal) -> bool,
    {
        let v = self.proposals.read().iter().find(|v| f(v)).cloned();
        Ok(v)
    }
}
