//! The consensus component. Provides distributed consensus among the nodes in the network.

#![warn(clippy::arithmetic_side_effects)]

mod cl_context;
mod config;
mod consensus_protocol;
mod era_supervisor;
#[macro_use]
pub mod highway_core;
pub(crate) mod error;
mod leader_sequence;
mod metrics;
pub mod protocols;
#[cfg(test)]
pub(crate) mod tests;
mod traits;
pub mod utils;
mod validator_change;

use std::{
    borrow::Cow,
    fmt::{self, Debug, Display, Formatter},
    sync::Arc,
    time::Duration,
};

use datasize::DataSize;
use derive_more::From;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, trace};

use casper_types::{EraId, Timestamp};

use crate::{
    components::Component,
    effect::{
        announcements::{
            ConsensusAnnouncement, FatalAnnouncement, MetaBlockAnnouncement,
            PeerBehaviorAnnouncement,
        },
        diagnostics_port::DumpConsensusStateRequest,
        incoming::{ConsensusDemand, ConsensusMessageIncoming},
        requests::{
            ChainspecRawBytesRequest, ConsensusRequest, ContractRuntimeRequest,
            DeployBufferRequest, NetworkInfoRequest, NetworkRequest,
            ProposedBlockValidationRequest, StorageRequest,
        },
        EffectBuilder, EffectExt, Effects,
    },
    failpoints::FailpointActivation,
    protocol::Message,
    reactor::ReactorEvent,
    types::{
        appendable_block::AddError, BlockHash, BlockHeader, BlockPayload, DeployHash,
        DeployOrTransferHash, NodeId,
    },
    NodeRng,
};
use protocols::{highway::HighwayProtocol, zug::Zug};
use traits::Context;

pub use cl_context::ClContext;
pub(crate) use config::{ChainspecConsensusExt, Config};
pub(crate) use consensus_protocol::{BlockContext, EraReport, ProposedBlock};
pub(crate) use era_supervisor::{debug::EraDump, EraSupervisor, SerializedMessage};
#[cfg(test)]
pub(crate) use highway_core::highway::Vertex as HighwayVertex;
pub(crate) use leader_sequence::LeaderSequence;
#[cfg(test)]
pub(crate) use protocols::highway::{max_rounds_per_era, HighwayMessage};
pub(crate) use validator_change::ValidatorChange;

const COMPONENT_NAME: &str = "consensus";

#[allow(clippy::arithmetic_side_effects)]
mod relaxed {
    // This module exists solely to exempt the `EnumDiscriminants` macro generated code from the
    // module-wide `clippy::arithmetic_side_effects` lint.

    use casper_types::{EraId, PublicKey};
    use datasize::DataSize;
    use serde::{Deserialize, Serialize};
    use strum::EnumDiscriminants;

    use super::era_supervisor::SerializedMessage;

    #[derive(DataSize, Clone, Serialize, Deserialize, EnumDiscriminants)]
    #[strum_discriminants(derive(strum::EnumIter))]
    pub(crate) enum ConsensusMessage {
        /// A protocol message, to be handled by the instance in the specified era.
        Protocol {
            era_id: EraId,
            payload: SerializedMessage,
        },
        /// A request for evidence against the specified validator, from any era that is still
        /// bonded in `era_id`.
        EvidenceRequest { era_id: EraId, pub_key: PublicKey },
    }
}
pub(crate) use relaxed::ConsensusMessage;

/// A request to be handled by the consensus protocol instance in a particular era.
#[derive(DataSize, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, From)]
pub(crate) enum EraRequest<C>
where
    C: Context,
{
    Zug(protocols::zug::SyncRequest<C>),
}

/// A protocol request message, to be handled by the instance in the specified era.
#[derive(DataSize, Clone, Serialize, Deserialize)]
pub(crate) struct ConsensusRequestMessage {
    era_id: EraId,
    payload: SerializedMessage,
}

/// An ID to distinguish different timers. What they are used for is specific to each consensus
/// protocol implementation.
#[derive(DataSize, Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TimerId(pub u8);

/// An ID to distinguish queued actions. What they are used for is specific to each consensus
/// protocol implementation.
#[derive(DataSize, Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ActionId(pub u8);

/// Payload for a block to be proposed.
#[derive(DataSize, Debug, From)]
pub struct NewBlockPayload {
    pub(crate) era_id: EraId,
    pub(crate) block_payload: Arc<BlockPayload>,
    pub(crate) block_context: BlockContext<ClContext>,
}

/// The result of validation of a ProposedBlock.
#[derive(DataSize, Debug, From)]
pub struct ValidationResult {
    era_id: EraId,
    sender: NodeId,
    proposed_block: ProposedBlock<ClContext>,
    error: Option<ValidationError>,
}

#[derive(Clone, DataSize, Debug, Error, Serialize)]
/// A proposed block validation error.
// TODO: This error probably needs to move to a different component.
pub enum ValidationError {
    /// A deploy hash in the proposed block has been found in an ancestor block.
    #[error("deploy hash {0} has been replayed")]
    ContainsReplayedDeploy(DeployHash),
    /// A deploy could not be fetched from any of the identified holders.
    #[error("exhausted potential holders of proposed block, missing {} deploys", missing_deploys.len())]
    ExhaustedBlockHolders {
        /// The deploys still missing.
        missing_deploys: Vec<DeployOrTransferHash>,
    },
    /// An already invalid block was submitted for validation.
    ///
    /// This is likely a bug in the node itself.
    #[error("validation of failed block, likely a bug")]
    ValidationOfFailedBlock,
    /// The submitted block is already in process of being validated.
    ///
    /// This is likely a bug, since no block should be submitted for validation twice.
    #[error("duplicate validation attempt, likely a bug")]
    DuplicateValidationAttempt,
    /// Found deploy in storage, but did not match the hash requested.
    ///
    /// This indicates a corrupted storage.
    // Note: It seems rather mean to ban peers for our own corrupted storage.
    #[error("local storage appears corrupted, deploy mismatch when asked for deploy {0}")]
    InternalDataCorruption(DeployOrTransferHash),
    /// The deploy we received
    ///
    /// This is likely a bug, since the deploy fetcher should ensure that this does not happen.
    #[error("received wrong or invalid deploy from peer when asked for deploy {0}")]
    WrongDeploySent(DeployOrTransferHash),
    /// A contained deploy has no valid deploy footprint.
    #[error("no valid deploy footprint for deploy {deploy_hash}: {error}")]
    DeployHasInvalidFootprint {
        /// Hash of deploy that failed.
        deploy_hash: DeployOrTransferHash,
        /// The error reported when trying to footprint it.
        // Note: The respective error is hard to serialize and make `Sync`-able, so it is inlined
        //       in string form here.
        error: String,
    },
    /// Too many non-transfer deploys in block.
    #[error("block exceeds limit of non-transfer deploys of {0}")]
    ExceedsNonTransferDeployLimit(usize),
    /// Too many non-transfer deploys in block.
    #[error("block exceeds limit of transfers of {0}")]
    ExceedsTransferLimit(usize),
    /// The approvals hash could not be serialized.
    // Note: `bytesrepr::Error` does not implement `std::error::Error`.
    #[error("failed to serialize approvals hash: {0}")]
    CannotSerializeApprovalsHash(String),
    /// A duplicated deploy was found within the block.
    #[error("duplicate deploy {0} in block")]
    DuplicateDeploy(DeployOrTransferHash),
    /// Exhausted all peers while trying to validate block.
    #[error("peers exhausted")]
    PeersExhausted,
    /// Failed to construct a `GetRequest`.
    #[error("could not construct GetRequest for {id}, peer {peer}")]
    CouldNotConstructGetRequest {
        /// The `GetRequest`'s ID, serialized as string
        id: String,
        /// The peer ID the `GetRequest` was directed at.
        peer: Box<NodeId>,
    },
    /// Validation data mismatch.
    #[error("validation data mismatch on {id}, peer {peer}")]
    ValidationMetadataMismatch {
        /// The item's ID for which validation data did not match.
        id: String,
        /// The peer ID involved.
        peer: Box<NodeId>,
    },
    /// The validation state was found to be `InProgress`.
    #[error("encountered in-progress validation state after completion, likely a bug")]
    InProgressAfterCompletion,
    /// A given deploy could not be included in the block by adding it to the appendable block.
    #[error("failed to include deploy {deploy_hash} in block")]
    DeployInclusionFailure {
        /// Hash of the deploy that was rejected.
        deploy_hash: DeployOrTransferHash,
        /// The underlying error of the appendable block.
        #[source]
        error: AddError,
    },
}

impl ValidationResult {
    /// Creates a new valid `ValidationResult`.
    #[inline(always)]
    fn new_valid(era_id: EraId, sender: NodeId, proposed_block: ProposedBlock<ClContext>) -> Self {
        Self {
            era_id,
            sender,
            proposed_block,
            error: None,
        }
    }

    /// Creates a new invalid `ValidationResult`.
    #[inline(always)]
    fn new_invalid(
        era_id: EraId,
        sender: NodeId,
        proposed_block: ProposedBlock<ClContext>,
        error: ValidationError,
    ) -> Self {
        Self {
            era_id,
            sender,
            proposed_block,
            error: Some(error),
        }
    }
}

/// Consensus component event.
#[derive(DataSize, Debug, From)]
pub(crate) enum Event {
    /// An incoming network message.
    #[from]
    Incoming(ConsensusMessageIncoming),
    /// A variant used with failpoints - when a message arrives, we fire this event with a delay,
    /// and it also causes the message to be handled.
    DelayedIncoming(ConsensusMessageIncoming),
    /// An incoming demand message.
    #[from]
    DemandIncoming(ConsensusDemand),
    /// A scheduled event to be handled by a specified era.
    Timer {
        era_id: EraId,
        timestamp: Timestamp,
        timer_id: TimerId,
    },
    /// A queued action to be handled by a specific era.
    Action { era_id: EraId, action_id: ActionId },
    /// We are receiving the data we require to propose a new block.
    NewBlockPayload(NewBlockPayload),
    #[from]
    ConsensusRequest(ConsensusRequest),
    /// A new block has been added to the linear chain.
    BlockAdded {
        header: Box<BlockHeader>,
        header_hash: BlockHash,
    },
    /// The proposed block has been validated.
    ResolveValidity(ValidationResult),
    /// Deactivate the era with the given ID, unless the number of faulty validators increases.
    DeactivateEra {
        era_id: EraId,
        faulty_num: usize,
        delay: Duration,
    },
    /// Dump state for debugging purposes.
    #[from]
    DumpState(DumpConsensusStateRequest),
}

impl Debug for ConsensusMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsensusMessage::Protocol { era_id, payload: _ } => {
                write!(f, "Protocol {{ era_id: {:?}, .. }}", era_id)
            }
            ConsensusMessage::EvidenceRequest { era_id, pub_key } => f
                .debug_struct("EvidenceRequest")
                .field("era_id", era_id)
                .field("pub_key", pub_key)
                .finish(),
        }
    }
}

impl Display for ConsensusMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ConsensusMessage::Protocol { era_id, payload } => {
                write!(
                    f,
                    "protocol message ({} bytes) in {}",
                    payload.as_raw().len(),
                    era_id
                )
            }
            ConsensusMessage::EvidenceRequest { era_id, pub_key } => write!(
                f,
                "request for evidence of fault by {} in {} or earlier",
                pub_key, era_id,
            ),
        }
    }
}

impl Debug for ConsensusRequestMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ConsensusRequestMessage {{ era_id: {:?}, .. }}",
            self.era_id
        )
    }
}

impl Display for ConsensusRequestMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "protocol request {:?} in {}", self.payload, self.era_id)
    }
}

impl Display for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Event::Incoming(ConsensusMessageIncoming {
                sender,
                message,
                ticket: _,
            }) => {
                write!(f, "message from {:?}: {}", sender, message)
            }
            Event::DelayedIncoming(ConsensusMessageIncoming {
                sender,
                message,
                ticket: _,
            }) => {
                write!(f, "delayed message from {:?}: {}", sender, message)
            }
            Event::DemandIncoming(demand) => {
                write!(f, "demand from {:?}: {}", demand.sender, demand.request_msg)
            }
            Event::Timer {
                era_id,
                timestamp,
                timer_id,
            } => write!(
                f,
                "timer (ID {}) for {} scheduled for timestamp {}",
                timer_id.0, era_id, timestamp,
            ),
            Event::Action { era_id, action_id } => {
                write!(f, "action (ID {}) for {}", action_id.0, era_id)
            }
            Event::NewBlockPayload(NewBlockPayload {
                era_id,
                block_payload,
                block_context,
            }) => write!(
                f,
                "New proposed block for era {:?}: {:?}, {:?}",
                era_id, block_payload, block_context
            ),
            Event::ConsensusRequest(request) => write!(
                f,
                "A request for consensus component hash been received: {:?}",
                request
            ),
            Event::BlockAdded {
                header: _,
                header_hash,
            } => write!(
                f,
                "A block has been added to the linear chain: {}",
                header_hash,
            ),
            Event::ResolveValidity(ValidationResult {
                era_id,
                sender,
                proposed_block,
                error,
            }) => {
                write!(
                    f,
                    "Proposed block received from {:?} for {} is ",
                    sender, era_id
                )?;

                if let Some(err) = error {
                    write!(f, "invalid ({})", err)?;
                } else {
                    f.write_str("valid")?;
                };

                write!(f, ": {:?}", proposed_block)?;

                Ok(())
            }
            Event::DeactivateEra {
                era_id, faulty_num, ..
            } => write!(
                f,
                "Deactivate old {} unless additional faults are observed; faults so far: {}",
                era_id, faulty_num
            ),
            Event::DumpState(req) => Display::fmt(req, f),
        }
    }
}

/// A helper trait whose bounds represent the requirements for a reactor event that `EraSupervisor`
/// can work with.
pub(crate) trait ReactorEventT:
    ReactorEvent
    + From<Event>
    + Send
    + From<NetworkRequest<Message>>
    + From<ConsensusDemand>
    + From<NetworkInfoRequest>
    + From<DeployBufferRequest>
    + From<ConsensusAnnouncement>
    + From<ProposedBlockValidationRequest>
    + From<StorageRequest>
    + From<ContractRuntimeRequest>
    + From<ChainspecRawBytesRequest>
    + From<PeerBehaviorAnnouncement>
    + From<MetaBlockAnnouncement>
    + From<FatalAnnouncement>
{
}

impl<REv> ReactorEventT for REv where
    REv: ReactorEvent
        + From<Event>
        + Send
        + From<ConsensusDemand>
        + From<NetworkRequest<Message>>
        + From<NetworkInfoRequest>
        + From<DeployBufferRequest>
        + From<ConsensusAnnouncement>
        + From<ProposedBlockValidationRequest>
        + From<StorageRequest>
        + From<ContractRuntimeRequest>
        + From<ChainspecRawBytesRequest>
        + From<PeerBehaviorAnnouncement>
        + From<MetaBlockAnnouncement>
        + From<FatalAnnouncement>
{
}

impl<REv> Component<REv> for EraSupervisor
where
    REv: ReactorEventT,
{
    type Event = Event;

    fn handle_event(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        rng: &mut NodeRng,
        event: Self::Event,
    ) -> Effects<Self::Event> {
        trace!("{:?}", event);
        match event {
            Event::Timer {
                era_id,
                timestamp,
                timer_id,
            } => self.handle_timer(effect_builder, rng, era_id, timestamp, timer_id),
            Event::Action { era_id, action_id } => {
                self.handle_action(effect_builder, rng, era_id, action_id)
            }
            Event::Incoming(ConsensusMessageIncoming {
                sender,
                message,
                ticket,
            }) => {
                let delay_by = self.message_delay_failpoint.fire(rng).cloned();
                if let Some(delay) = delay_by {
                    effect_builder
                        .set_timeout(Duration::from_millis(delay))
                        .event(move |_| {
                            Event::DelayedIncoming(ConsensusMessageIncoming {
                                sender,
                                message,
                                ticket,
                            })
                        })
                } else {
                    let rv = self.handle_message(effect_builder, rng, sender, *message);
                    drop(ticket); // TODO: drop ticket in `handle_message` effect instead
                    rv
                }
            }
            Event::DelayedIncoming(ConsensusMessageIncoming {
                sender,
                message,
                ticket: _, // TODO: drop ticket in `handle_message` effect instead
            }) => self.handle_message(effect_builder, rng, sender, *message),
            Event::DemandIncoming(ConsensusDemand {
                sender,
                request_msg: demand,
                auto_closing_responder,
            }) => self.handle_demand(effect_builder, rng, sender, demand, auto_closing_responder),
            Event::NewBlockPayload(new_block_payload) => {
                self.handle_new_block_payload(effect_builder, rng, new_block_payload)
            }
            Event::BlockAdded {
                header,
                header_hash: _,
            } => self.handle_block_added(effect_builder, rng, *header),
            Event::ResolveValidity(resolve_validity) => {
                self.resolve_validity(effect_builder, rng, resolve_validity)
            }
            Event::DeactivateEra {
                era_id,
                faulty_num,
                delay,
            } => self.handle_deactivate_era(effect_builder, era_id, faulty_num, delay),
            Event::ConsensusRequest(ConsensusRequest::Status(responder)) => self.status(responder),
            Event::ConsensusRequest(ConsensusRequest::ValidatorChanges(responder)) => {
                let validator_changes = self.get_validator_changes();
                responder.respond(validator_changes).ignore()
            }
            Event::DumpState(req @ DumpConsensusStateRequest { era_id, .. }) => {
                let current_era = match self.current_era() {
                    None => {
                        return req
                            .answer(Err(Cow::Owned("consensus not initialized".to_string())))
                            .ignore()
                    }
                    Some(era_id) => era_id,
                };

                let requested_era = era_id.unwrap_or(current_era);

                // We emit some log message to get some performance information and give the
                // operator a chance to find out why their node is busy.
                info!(era_id=%requested_era.value(), was_latest=era_id.is_none(), "dumping era via diagnostics port");

                let era_dump_result = self
                    .open_eras()
                    .get(&requested_era)
                    .ok_or_else(|| {
                        Cow::Owned(format!(
                            "could not dump consensus, {} not found",
                            requested_era
                        ))
                    })
                    .and_then(|era| EraDump::dump_era(era, requested_era));

                match era_dump_result {
                    Ok(dump) => req.answer(Ok(&dump)).ignore(),
                    Err(err) => req.answer(Err(err)).ignore(),
                }
            }
        }
    }

    fn name(&self) -> &str {
        COMPONENT_NAME
    }

    fn activate_failpoint(&mut self, activation: &FailpointActivation) {
        self.message_delay_failpoint.update_from(activation);
        self.proposal_delay_failpoint.update_from(activation);
    }
}
