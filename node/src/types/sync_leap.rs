use std::{
    collections::{BTreeMap, HashSet},
    fmt::{self, Display, Formatter},
    iter,
};

use datasize::DataSize;
use itertools::Itertools;
use num_rational::Ratio;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use casper_types::{crypto, EraId};
use tracing::error;

use crate::{
    components::fetcher::{FetchItem, Tag},
    types::{
        error::BlockHeaderWithMetadataValidationError, BlockHash, BlockHeader,
        BlockHeaderWithMetadata, BlockSignatures, EraValidatorWeights,
    },
    utils::{self, BlockSignatureError},
};

use super::sync_leap_validation_metadata::SyncLeapValidationMetaData;

#[derive(Error, Debug)]
pub(crate) enum SyncLeapValidationError {
    #[error("No ancestors of the trusted block provided.")]
    MissingTrustedAncestors,
    #[error("The SyncLeap does not contain proof that all its headers are on the right chain.")]
    IncompleteProof,
    #[error(transparent)]
    HeadersNotSufficientlySigned(BlockSignatureError),
    #[error("The block signatures are not cryptographically valid: {0}")]
    Crypto(crypto::Error),
    #[error(transparent)]
    BlockWithMetadata(BlockHeaderWithMetadataValidationError),
    #[error("Too many switch blocks: leaping across that many eras is not allowed.")]
    TooManySwitchBlocks,
    #[error("Trusted ancestor headers must be in reverse chronological order.")]
    TrustedAncestorsNotSorted,
    #[error("Last trusted ancestor is not a switch block.")]
    MissingAncestorSwitchBlock,
    #[error(
        "Only the last trusted ancestor is allowed to be a switch block or the genesis block."
    )]
    UnexpectedAncestorSwitchBlock,
    #[error("Signed block headers present despite trusted_ancestor_only flag.")]
    UnexpectedSignedBlockHeaders,
}

/// Identifier for a SyncLeap.
#[derive(Debug, Serialize, Deserialize, Copy, Clone, Hash, PartialEq, Eq, DataSize)]
pub(crate) struct SyncLeapIdentifier {
    /// The block hash of the initial trusted block.
    block_hash: BlockHash,
    /// If true, signed_block_headers are not required.
    trusted_ancestor_only: bool,
}

impl SyncLeapIdentifier {
    pub(crate) fn sync_to_tip(block_hash: BlockHash) -> Self {
        SyncLeapIdentifier {
            block_hash,
            trusted_ancestor_only: false,
        }
    }

    pub(crate) fn sync_to_historical(block_hash: BlockHash) -> Self {
        SyncLeapIdentifier {
            block_hash,
            trusted_ancestor_only: true,
        }
    }

    pub(crate) fn block_hash(&self) -> BlockHash {
        self.block_hash
    }

    pub(crate) fn trusted_ancestor_only(&self) -> bool {
        self.trusted_ancestor_only
    }
}

impl Display for SyncLeapIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} trusted_ancestor_only: {}",
            self.block_hash, self.trusted_ancestor_only
        )
    }
}

/// Headers and signatures required to prove that if a given trusted block hash is on the correct
/// chain, then so is a later header, which should be the most recent one according to the sender.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, DataSize)]
pub(crate) struct SyncLeap {
    /// Requester indicates if they want only the header and ancestor headers,
    /// of if they want everything.
    pub trusted_ancestor_only: bool,
    /// The header of the trusted block specified by hash by the requester.
    pub trusted_block_header: BlockHeader,
    /// The block headers of the trusted block's ancestors, back to the most recent switch block.
    pub trusted_ancestor_headers: Vec<BlockHeader>,
    /// The headers of all switch blocks known to the sender, after the trusted block but before
    /// their highest block, with signatures, plus the signed highest block.
    pub signed_block_headers: Vec<BlockHeaderWithMetadata>,
}

impl SyncLeap {
    pub(crate) fn era_validator_weights(
        &self,
        fault_tolerance_fraction: Ratio<u64>,
    ) -> impl Iterator<Item = EraValidatorWeights> + '_ {
        let switch_block_heights: HashSet<_> = self
            .switch_blocks_headers()
            .map(BlockHeader::height)
            .collect();
        self.switch_blocks_headers()
            .find(|block_header| block_header.is_genesis())
            .into_iter()
            .flat_map(move |block_header| {
                Some(EraValidatorWeights::new(
                    EraId::default(),
                    block_header.next_era_validator_weights().cloned()?,
                    fault_tolerance_fraction,
                ))
            })
            .chain(
                self.switch_blocks_headers()
                    // filter out switch blocks preceding immediate switch blocks - we don't want
                    // to read the era validators directly from them, as they might have been
                    // altered by the upgrade, we'll get them from the blocks' global states
                    // instead
                    .filter(move |block_header| {
                        !switch_block_heights.contains(&(block_header.height() + 1))
                    })
                    .flat_map(move |block_header| {
                        Some(EraValidatorWeights::new(
                            block_header.next_block_era_id(),
                            block_header.next_era_validator_weights().cloned()?,
                            fault_tolerance_fraction,
                        ))
                    }),
            )
    }

    pub(crate) fn highest_block_height(&self) -> u64 {
        self.headers()
            .map(BlockHeader::height)
            .max()
            .unwrap_or_else(|| self.trusted_block_header.height())
    }

    pub(crate) fn highest_block_header_and_signatures(
        &self,
    ) -> (&BlockHeader, Option<&BlockSignatures>) {
        let header = self
            .headers()
            .max_by_key(|header| header.height())
            .unwrap_or(&self.trusted_block_header);
        let signatures = self
            .signed_block_headers
            .iter()
            .find(|block_header_with_metadata| {
                block_header_with_metadata.block_header.height() == header.height()
            })
            .map(|block_header_with_metadata| &block_header_with_metadata.block_signatures);
        (header, signatures)
    }

    pub(crate) fn highest_block_hash(&self) -> BlockHash {
        self.highest_block_header_and_signatures().0.block_hash()
    }

    pub(crate) fn headers(&self) -> impl Iterator<Item = &BlockHeader> {
        iter::once(&self.trusted_block_header)
            .chain(&self.trusted_ancestor_headers)
            .chain(self.signed_block_headers.iter().map(|sh| &sh.block_header))
    }

    pub(crate) fn switch_blocks_headers(&self) -> impl Iterator<Item = &BlockHeader> {
        self.headers().filter(|header| header.is_switch_block())
    }
}

impl Display for SyncLeap {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "sync leap message for trusted {}",
            self.trusted_block_header.block_hash()
        )
    }
}

impl FetchItem for SyncLeap {
    type Id = SyncLeapIdentifier;
    type ValidationError = SyncLeapValidationError;
    type ValidationMetadata = SyncLeapValidationMetaData;

    const TAG: Tag = Tag::SyncLeap;

    fn fetch_id(&self) -> Self::Id {
        SyncLeapIdentifier {
            block_hash: self.trusted_block_header.block_hash(),
            trusted_ancestor_only: self.trusted_ancestor_only,
        }
    }

    fn validate(
        &self,
        validation_metadata: &SyncLeapValidationMetaData,
    ) -> Result<(), Self::ValidationError> {
        if self.trusted_ancestor_headers.is_empty() && self.trusted_block_header.height() > 0 {
            return Err(SyncLeapValidationError::MissingTrustedAncestors);
        }
        if self.signed_block_headers.len() as u64
            > validation_metadata.recent_era_count.saturating_add(1)
        {
            return Err(SyncLeapValidationError::TooManySwitchBlocks);
        }
        if self
            .trusted_ancestor_headers
            .iter()
            .tuple_windows()
            .any(|(child, parent)| *child.parent_hash() != parent.block_hash())
        {
            return Err(SyncLeapValidationError::TrustedAncestorsNotSorted);
        }
        let mut trusted_ancestor_iter = self.trusted_ancestor_headers.iter().rev();
        if let Some(last_ancestor) = trusted_ancestor_iter.next() {
            if !last_ancestor.is_switch_block() && !last_ancestor.is_genesis() {
                return Err(SyncLeapValidationError::MissingAncestorSwitchBlock);
            }
        }
        if trusted_ancestor_iter.any(BlockHeader::is_switch_block) {
            return Err(SyncLeapValidationError::UnexpectedAncestorSwitchBlock);
        }
        if self.trusted_ancestor_only && !self.signed_block_headers.is_empty() {
            return Err(SyncLeapValidationError::UnexpectedSignedBlockHeaders);
        }

        let mut headers: BTreeMap<BlockHash, &BlockHeader> = self
            .headers()
            .map(|header| (header.block_hash(), header))
            .collect();
        let mut signatures: BTreeMap<EraId, Vec<&BlockSignatures>> = BTreeMap::new();
        for signed_header in &self.signed_block_headers {
            signatures
                .entry(signed_header.block_signatures.era_id)
                .or_default()
                .push(&signed_header.block_signatures);
        }

        let mut headers_with_sufficient_finality: Vec<BlockHash> =
            vec![self.trusted_block_header.block_hash()];

        while let Some(hash) = headers_with_sufficient_finality.pop() {
            if let Some(header) = headers.remove(&hash) {
                headers_with_sufficient_finality.push(*header.parent_hash());
                if let Some(mut validator_weights) = header.next_era_validator_weights() {
                    // If this is a switch block right before the upgrade to the current protocol
                    // version, and if this upgrade changes the validator set, use the validator
                    // weights from the chainspec.
                    if header.next_block_era_id() == validation_metadata.activation_point.era_id() {
                        if let Some(updated_weights) = validation_metadata
                            .global_state_update
                            .as_ref()
                            .and_then(|update| update.validators.as_ref())
                        {
                            validator_weights = updated_weights
                        }
                    }

                    if let Some(era_sigs) = signatures.remove(&header.next_block_era_id()) {
                        for sigs in era_sigs {
                            if let Err(err) = utils::check_sufficient_block_signatures(
                                validator_weights,
                                validation_metadata.finality_threshold_fraction,
                                Some(sigs),
                            ) {
                                return Err(SyncLeapValidationError::HeadersNotSufficientlySigned(
                                    err,
                                ));
                            }
                            headers_with_sufficient_finality.push(sigs.block_hash);
                        }
                    }
                }
            }
        }

        // any orphaned headers == incomplete proof
        let incomplete_headers_proof = !headers.is_empty();
        // any orphaned signatures == incomplete proof
        let incomplete_signatures_proof = !signatures.is_empty();

        if incomplete_headers_proof || incomplete_signatures_proof {
            return Err(SyncLeapValidationError::IncompleteProof);
        }

        for signed_header in &self.signed_block_headers {
            signed_header
                .validate()
                .map_err(SyncLeapValidationError::BlockWithMetadata)?;
        }

        // defer cryptographic verification until last to avoid unnecessary computation
        for signed_header in &self.signed_block_headers {
            signed_header
                .block_signatures
                .verify()
                .map_err(SyncLeapValidationError::Crypto)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // The `FetchItem::<SyncLeap>::validate()` function can potentially return the
    // `SyncLeapValidationError::BlockWithMetadata` error as a result of calling
    // `BlockHeaderWithMetadata::validate()`, but in practice this will always be detected earlier
    // as an `SyncLeapValidationError::IncompleteProof` error. Hence, there is no explicit test for
    // `SyncLeapValidationError::BlockWithMetadata`.

    use std::{
        collections::{BTreeMap, BTreeSet},
        iter,
    };

    use casper_types::{
        crypto, testing::TestRng, EraId, ProtocolVersion, PublicKey, SecretKey, Signature,
        Timestamp, U512,
    };
    use num_rational::Ratio;
    use rand::Rng;

    use super::SyncLeap;
    use crate::{
        components::fetcher::FetchItem,
        types::{
            chainspec::GlobalStateUpdate, sync_leap::SyncLeapValidationError,
            sync_leap_validation_metadata::SyncLeapValidationMetaData, ActivationPoint, Block,
            BlockHash, BlockHeader, BlockHeaderWithMetadata, BlockSignatures, EraValidatorWeights,
            FinalitySignature, FinalizedBlock, SyncLeapIdentifier,
        },
        utils::BlockSignatureError,
    };

    fn random_block_at_height(rng: &mut TestRng, height: u64) -> Block {
        let era_id = rng.gen();
        let protocol_version = ProtocolVersion::default();
        let is_switch = rng.gen();

        Block::random_with_specifics(
            rng,
            era_id,
            height,
            protocol_version,
            is_switch,
            iter::empty(),
        )
    }

    fn random_switch_block_at_height_and_era(
        rng: &mut TestRng,
        height: u64,
        era_id: EraId,
    ) -> Block {
        let protocol_version = ProtocolVersion::default();
        let is_switch = true;

        Block::random_with_specifics(
            rng,
            era_id,
            height,
            protocol_version,
            is_switch,
            iter::empty(),
        )
    }

    fn make_signed_block_header_from_height(
        height: usize,
        test_chain: &[Block],
        validators: &[ValidatorSpec],
        add_proofs: bool,
    ) -> BlockHeaderWithMetadata {
        let header = test_chain.get(height).unwrap().header().clone();
        make_signed_block_header_from_header(&header, validators, add_proofs)
    }

    fn make_signed_block_header_from_header(
        block_header: &BlockHeader,
        validators: &[ValidatorSpec],
        add_proofs: bool,
    ) -> BlockHeaderWithMetadata {
        let hash = block_header.block_hash();
        let era_id = block_header.era_id();
        let mut block_signatures = BlockSignatures::new(hash, era_id);
        validators.iter().for_each(
            |ValidatorSpec {
                 secret_key,
                 public_key,
                 weight: _,
             }| {
                let finality_signature =
                    FinalitySignature::create(hash, era_id, secret_key, public_key.clone());
                if add_proofs {
                    block_signatures.insert_proof(public_key.clone(), finality_signature.signature);
                }
            },
        );

        BlockHeaderWithMetadata {
            block_header: block_header.clone(),
            block_signatures,
        }
    }

    // Each generated era gets two validators pulled from the provided `validators` set.
    fn make_test_sync_leap_with_validators(
        rng: &mut TestRng,
        validators: &[ValidatorSpec],
        switch_blocks: &[u64],
        query: usize,
        trusted_ancestor_headers: &[usize],
        signed_block_headers: &[usize],
        add_proofs: bool,
    ) -> SyncLeap {
        let mut test_chain_spec = TestChainSpec::new(rng, Some(switch_blocks.to_vec()), validators);
        let test_chain: Vec<_> = test_chain_spec.iter().take(12).collect();

        let trusted_block_header = test_chain.get(query).unwrap().header().clone();

        let trusted_ancestor_headers: Vec<_> = trusted_ancestor_headers
            .iter()
            .map(|height| test_chain.get(*height).unwrap().header().clone())
            .collect();

        let signed_block_headers: Vec<_> = signed_block_headers
            .iter()
            .map(|height| {
                make_signed_block_header_from_height(*height, &test_chain, validators, add_proofs)
            })
            .collect();

        SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header,
            trusted_ancestor_headers,
            signed_block_headers,
        }
    }

    fn make_test_sync_leap(
        rng: &mut TestRng,
        switch_blocks: &[u64],
        query: usize,
        trusted_ancestor_headers: &[usize],
        signed_block_headers: &[usize],
        add_proofs: bool,
    ) -> SyncLeap {
        const DEFAULT_VALIDATOR_WEIGHT: u32 = 100;

        let validators: Vec<_> = iter::repeat_with(crypto::generate_ed25519_keypair)
            .take(2)
            .map(|(secret_key, public_key)| ValidatorSpec {
                secret_key,
                public_key,
                weight: Some(DEFAULT_VALIDATOR_WEIGHT.into()),
            })
            .collect();
        make_test_sync_leap_with_validators(
            rng,
            &validators,
            switch_blocks,
            query,
            trusted_ancestor_headers,
            signed_block_headers,
            add_proofs,
        )
    }

    fn test_sync_leap_validation_metadata() -> SyncLeapValidationMetaData {
        let unbonding_delay = 7;
        let auction_delay = 1;
        let activation_point = ActivationPoint::EraId(3000.into());
        let finality_threshold_fraction = Ratio::new(1, 3);

        SyncLeapValidationMetaData::new(
            unbonding_delay - auction_delay, // As per `CoreConfig::recent_era_count()`.
            activation_point,
            None,
            finality_threshold_fraction,
        )
    }

    #[test]
    fn should_validate_correct_sync_leap() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];
        let validation_metadata = test_sync_leap_validation_metadata();

        let mut rng = TestRng::new();

        // Querying for a non-switch block.
        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        let result = sync_leap.validate(&validation_metadata);
        assert!(result.is_ok());

        // Querying for a switch block.
        let query = 6;
        let trusted_ancestor_headers = [5, 4, 3];
        let signed_block_headers = [9, 11];
        let add_proofs = true;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        let result = sync_leap.validate(&validation_metadata);
        assert!(result.is_ok());
    }

    #[test]
    fn should_check_trusted_ancestors() {
        let mut rng = TestRng::new();
        let validation_metadata = test_sync_leap_validation_metadata();

        // Trusted ancestors can't be empty when trusted block height is greater than 0.
        let block = random_block_at_height(&mut rng, 1);

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: block.take_header(),
            trusted_ancestor_headers: Default::default(),
            signed_block_headers: Default::default(),
        };
        let result = sync_leap.validate(&validation_metadata);
        assert!(matches!(
            result,
            Err(SyncLeapValidationError::MissingTrustedAncestors)
        ));

        // When trusted block height is 0 and trusted ancestors are empty, validate
        // should yield a result different than `SyncLeapValidationError::MissingTrustedAncestors`.
        let block = random_block_at_height(&mut rng, 0);

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: block.take_header(),
            trusted_ancestor_headers: Default::default(),
            signed_block_headers: Default::default(),
        };
        let result = sync_leap.validate(&validation_metadata);
        assert!(!matches!(
            result,
            Err(SyncLeapValidationError::MissingTrustedAncestors)
        ));
    }

    #[test]
    fn should_check_signed_block_headers_size() {
        let mut rng = TestRng::new();
        let validation_metadata = test_sync_leap_validation_metadata();

        let max_allowed_size = validation_metadata.recent_era_count + 1;

        // Max allowed size should NOT trigger the `TooManySwitchBlocks` error.
        let generated_block_count = max_allowed_size;

        let block = random_block_at_height(&mut rng, 0);
        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: block.take_header(),
            trusted_ancestor_headers: Default::default(),
            signed_block_headers: std::iter::repeat_with(|| {
                let block = Block::random(&mut rng);
                let hash = block.hash();
                BlockHeaderWithMetadata {
                    block_header: block.header().clone(),
                    block_signatures: BlockSignatures::new(*hash, 0.into()),
                }
            })
            .take(generated_block_count as usize)
            .collect(),
        };
        let result = sync_leap.validate(&validation_metadata);
        assert!(!matches!(
            result,
            Err(SyncLeapValidationError::TooManySwitchBlocks)
        ));

        // Generating one more block should trigger the `TooManySwitchBlocks` error.
        let generated_block_count = max_allowed_size + 1;

        let block = random_block_at_height(&mut rng, 0);
        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: block.take_header(),
            trusted_ancestor_headers: Default::default(),
            signed_block_headers: std::iter::repeat_with(|| {
                let block = Block::random(&mut rng);
                let hash = block.hash();
                BlockHeaderWithMetadata {
                    block_header: block.header().clone(),
                    block_signatures: BlockSignatures::new(*hash, 0.into()),
                }
            })
            .take(generated_block_count as usize)
            .collect(),
        };
        let result = sync_leap.validate(&validation_metadata);
        assert!(matches!(
            result,
            Err(SyncLeapValidationError::TooManySwitchBlocks)
        ));
    }

    #[test]
    fn should_detect_unsorted_trusted_ancestors() {
        let mut rng = TestRng::new();
        let validation_metadata = test_sync_leap_validation_metadata();

        // Test block iterator produces blocks in order, however, the `trusted_ancestor_headers` is
        // expected to be sorted backwards (from the most recent ancestor back to the switch block).
        // Therefore, the generated blocks should cause the `TrustedAncestorsNotSorted` error to be
        // triggered.
        let block = random_block_at_height(&mut rng, 0);
        let block_iterator =
            TestBlockIterator::new(block.clone(), &mut rng, None, Default::default());

        let trusted_ancestor_headers = block_iterator
            .take(3)
            .map(|block| block.take_header())
            .collect();

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: block.take_header(),
            trusted_ancestor_headers,
            signed_block_headers: Default::default(),
        };
        let result = sync_leap.validate(&validation_metadata);
        assert!(matches!(
            result,
            Err(SyncLeapValidationError::TrustedAncestorsNotSorted)
        ));

        // Single trusted ancestor header it should never trigger the `TrustedAncestorsNotSorted`
        // error.
        let block = random_block_at_height(&mut rng, 0);
        let block_iterator =
            TestBlockIterator::new(block.clone(), &mut rng, None, Default::default());

        let trusted_ancestor_headers = block_iterator
            .take(1)
            .map(|block| block.take_header())
            .collect();

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: block.take_header(),
            trusted_ancestor_headers,
            signed_block_headers: Default::default(),
        };
        let result = sync_leap.validate(&validation_metadata);
        assert!(!matches!(
            result,
            Err(SyncLeapValidationError::TrustedAncestorsNotSorted)
        ));
    }

    #[test]
    fn should_detect_missing_ancestor_switch_block() {
        let mut rng = TestRng::new();
        let validation_metadata = test_sync_leap_validation_metadata();

        // Make sure `TestBlockIterator` creates no switch blocks.
        let switch_blocks = None;

        let block = random_block_at_height(&mut rng, 0);
        let block_iterator =
            TestBlockIterator::new(block.clone(), &mut rng, switch_blocks, Default::default());

        let trusted_ancestor_headers: Vec<_> = block_iterator
            .take(3)
            .map(|block| block.take_header())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: block.take_header(),
            trusted_ancestor_headers,
            signed_block_headers: Default::default(),
        };
        let result = sync_leap.validate(&validation_metadata);
        assert!(matches!(
            result,
            Err(SyncLeapValidationError::MissingAncestorSwitchBlock)
        ));
    }

    #[test]
    fn should_detect_unexpected_ancestor_switch_block() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S       S   S           S           S
        let switch_blocks = [0, 2, 3, 6, 9];
        let validation_metadata = test_sync_leap_validation_metadata();

        let mut rng = TestRng::new();

        // Intentionally include two consecutive switch blocks (3, 2) in the
        // `trusted_ancestor_headers`, which should trigger the error.
        let trusted_ancestor_headers = [4, 3, 2];

        let query = 5;
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        let result = sync_leap.validate(&validation_metadata);
        assert!(matches!(
            result,
            Err(SyncLeapValidationError::UnexpectedAncestorSwitchBlock)
        ));
    }

    #[test]
    fn should_detect_unexpected_signed_block_header() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];
        let validation_metadata = test_sync_leap_validation_metadata();

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let mut sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        // When `trusted_ancestor_only` we expect an error when `signed_block_headers` is not empty.
        sync_leap.trusted_ancestor_only = true;

        let result = sync_leap.validate(&validation_metadata);
        assert!(matches!(
            result,
            Err(SyncLeapValidationError::UnexpectedSignedBlockHeaders)
        ));
    }

    #[test]
    fn should_detect_not_sufficiently_signed_headers() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];
        let validation_metadata = test_sync_leap_validation_metadata();

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = false;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        let result = sync_leap.validate(&validation_metadata);
        assert!(
            matches!(result, Err(SyncLeapValidationError::HeadersNotSufficientlySigned(inner))
             if matches!(&inner, BlockSignatureError::InsufficientWeightForFinality{
                trusted_validator_weights: _,
                block_signatures: _,
                signature_weight,
                total_validator_weight:_,
                fault_tolerance_fraction:_ } if signature_weight == &Some(Box::new(0.into()))))
        );
    }

    #[test]
    fn should_detect_orphaned_headers() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];
        let validation_metadata = test_sync_leap_validation_metadata();

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let mut sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        // Add single orphaned block. Signatures are cloned from a legit block to avoid bailing on
        // the signature validation check.
        let orphaned_block = Block::random(&mut rng);
        let orphaned_block_with_metadata = BlockHeaderWithMetadata {
            block_header: orphaned_block.header().clone(),
            block_signatures: sync_leap
                .signed_block_headers
                .first()
                .unwrap()
                .block_signatures
                .clone(),
        };
        sync_leap
            .signed_block_headers
            .push(orphaned_block_with_metadata);

        let result = sync_leap.validate(&validation_metadata);
        assert!(matches!(
            result,
            Err(SyncLeapValidationError::IncompleteProof)
        ));
    }

    #[test]
    fn should_detect_orphaned_signatures() {
        const NON_EXISTING_ERA: u64 = u64::MAX;

        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];
        let validation_metadata = test_sync_leap_validation_metadata();

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let mut sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        // Insert signature from an era nowhere near the sync leap data. Base it on one of the
        // existing signatures to avoid bailing on the signature validation check.
        let mut signed_block_header = sync_leap.signed_block_headers.first_mut().unwrap().clone();
        signed_block_header.block_signatures.era_id = NON_EXISTING_ERA.into();
        sync_leap.signed_block_headers.push(signed_block_header);

        let result = sync_leap.validate(&validation_metadata);
        assert!(matches!(
            result,
            Err(SyncLeapValidationError::IncompleteProof)
        ));
    }

    #[test]
    fn should_fail_when_signature_fails_crypto_verification() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];
        let validation_metadata = test_sync_leap_validation_metadata();

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let mut sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        let mut signed_block_header = sync_leap.signed_block_headers.pop().unwrap();

        // Remove one correct proof.
        let proof = signed_block_header
            .block_signatures
            .proofs
            .pop_last()
            .unwrap();
        let validator_public_key = proof.0;

        // Create unverifiable signature (`Signature::System`).
        let finality_signature = FinalitySignature::new(
            signed_block_header.block_header.block_hash(),
            signed_block_header.block_header.era_id(),
            Signature::System,
            validator_public_key.clone(),
        );

        // Sneak it into the sync leap.
        signed_block_header
            .block_signatures
            .proofs
            .insert(validator_public_key, finality_signature.signature);
        sync_leap.signed_block_headers.push(signed_block_header);

        let result = sync_leap.validate(&validation_metadata);
        assert!(matches!(result, Err(SyncLeapValidationError::Crypto(_))));
    }

    #[test]
    fn should_use_correct_validator_weights_on_upgrade() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];

        const INDEX_OF_THE_LAST_SWITCH_BLOCK: usize = 1;
        let signed_block_headers = [6, 9, 11];

        let add_proofs = true;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        // Setup upgrade after the last switch block.
        let upgrade_block = sync_leap
            .signed_block_headers
            .get(INDEX_OF_THE_LAST_SWITCH_BLOCK)
            .unwrap();
        let upgrade_era = upgrade_block.block_header.era_id().successor();
        let activation_point = ActivationPoint::EraId(upgrade_era);

        // Set up validator change.
        const DEFAULT_VALIDATOR_WEIGHT: u64 = 100;
        let new_validators: BTreeMap<_, _> = iter::repeat_with(crypto::generate_ed25519_keypair)
            .take(2)
            .map(|(_, public_key)| (public_key, DEFAULT_VALIDATOR_WEIGHT.into()))
            .collect();
        let global_state_update = GlobalStateUpdate {
            validators: Some(new_validators),
            entries: Default::default(),
        };

        let unbonding_delay = 7;
        let auction_delay = 1;
        let finality_threshold_fraction = Ratio::new(1, 3);
        let validation_metadata = SyncLeapValidationMetaData::new(
            unbonding_delay - auction_delay, // As per `CoreConfig::recent_era_count()`.
            activation_point,
            Some(global_state_update),
            finality_threshold_fraction,
        );

        let result = sync_leap.validate(&validation_metadata);

        // By asserting on the `HeadersNotSufficientlySigned` error (with bogus validators set to
        // the original validators from the chain) we can prove that the validators smuggled in the
        // validation metadata were actually used in the verification process.
        let expected_bogus_validators: Vec<_> = sync_leap
            .signed_block_headers
            .last()
            .unwrap()
            .block_signatures
            .proofs
            .keys()
            .cloned()
            .collect();
        assert!(
            matches!(result, Err(SyncLeapValidationError::HeadersNotSufficientlySigned(inner))
             if matches!(&inner, BlockSignatureError::BogusValidators{
                trusted_validator_weights: _,
                block_signatures: _,
                bogus_validators
            } if bogus_validators == &expected_bogus_validators))
        );
    }

    #[test]
    fn should_return_headers() {
        let mut rng = TestRng::new();

        let trusted_block = Block::random_non_switch_block(&mut rng);

        let trusted_ancestor_1 = Block::random_switch_block(&mut rng);
        let trusted_ancestor_2 = Block::random_non_switch_block(&mut rng);
        let trusted_ancestor_3 = Block::random_non_switch_block(&mut rng);

        let signed_block_1 = Block::random_switch_block(&mut rng);
        let signed_block_2 = Block::random_switch_block(&mut rng);
        let signed_block_3 = Block::random_non_switch_block(&mut rng);
        let signed_block_header_with_metadata_1 =
            make_signed_block_header_from_header(signed_block_1.header(), &[], false);
        let signed_block_header_with_metadata_2 =
            make_signed_block_header_from_header(signed_block_2.header(), &[], false);
        let signed_block_header_with_metadata_3 =
            make_signed_block_header_from_header(signed_block_3.header(), &[], false);

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: trusted_block.header().clone(),
            trusted_ancestor_headers: vec![
                trusted_ancestor_1.header().clone(),
                trusted_ancestor_2.header().clone(),
                trusted_ancestor_3.header().clone(),
            ],
            signed_block_headers: vec![
                signed_block_header_with_metadata_1,
                signed_block_header_with_metadata_2,
                signed_block_header_with_metadata_3,
            ],
        };

        let actual_headers: BTreeSet<_> = sync_leap
            .headers()
            .map(|header| header.block_hash())
            .collect();
        let expected_headers: BTreeSet<_> = [
            trusted_block,
            trusted_ancestor_1,
            trusted_ancestor_2,
            trusted_ancestor_3,
            signed_block_1,
            signed_block_2,
            signed_block_3,
        ]
        .iter()
        .map(|block| *block.hash())
        .collect();
        assert_eq!(expected_headers, actual_headers);
    }

    #[test]
    fn should_return_switch_block_headers() {
        let mut rng = TestRng::new();

        let trusted_block = Block::random_non_switch_block(&mut rng);

        let trusted_ancestor_1 = Block::random_switch_block(&mut rng);
        let trusted_ancestor_2 = Block::random_non_switch_block(&mut rng);
        let trusted_ancestor_3 = Block::random_non_switch_block(&mut rng);

        let signed_block_1 = Block::random_switch_block(&mut rng);
        let signed_block_2 = Block::random_switch_block(&mut rng);
        let signed_block_3 = Block::random_non_switch_block(&mut rng);
        let signed_block_header_with_metadata_1 =
            make_signed_block_header_from_header(signed_block_1.header(), &[], false);
        let signed_block_header_with_metadata_2 =
            make_signed_block_header_from_header(signed_block_2.header(), &[], false);
        let signed_block_header_with_metadata_3 =
            make_signed_block_header_from_header(signed_block_3.header(), &[], false);

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: trusted_block.header().clone(),
            trusted_ancestor_headers: vec![
                trusted_ancestor_1.header().clone(),
                trusted_ancestor_2.header().clone(),
                trusted_ancestor_3.header().clone(),
            ],
            signed_block_headers: vec![
                signed_block_header_with_metadata_1.clone(),
                signed_block_header_with_metadata_2.clone(),
                signed_block_header_with_metadata_3.clone(),
            ],
        };

        let actual_headers: BTreeSet<_> = sync_leap
            .switch_blocks_headers()
            .map(|header| header.block_hash())
            .collect();
        let expected_headers: BTreeSet<_> = [
            trusted_ancestor_1.clone(),
            signed_block_1.clone(),
            signed_block_2.clone(),
        ]
        .iter()
        .map(|block| *block.hash())
        .collect();
        assert_eq!(expected_headers, actual_headers);

        // Also test when the trusted block is a switch block.
        let trusted_block = Block::random_switch_block(&mut rng);
        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: trusted_block.header().clone(),
            trusted_ancestor_headers: vec![
                trusted_ancestor_1.header().clone(),
                trusted_ancestor_2.header().clone(),
                trusted_ancestor_3.header().clone(),
            ],
            signed_block_headers: vec![
                signed_block_header_with_metadata_1,
                signed_block_header_with_metadata_2,
                signed_block_header_with_metadata_3,
            ],
        };
        let actual_headers: BTreeSet<_> = sync_leap
            .switch_blocks_headers()
            .map(|header| header.block_hash())
            .collect();
        let expected_headers: BTreeSet<_> = [
            trusted_block,
            trusted_ancestor_1,
            signed_block_1,
            signed_block_2,
        ]
        .iter()
        .map(|block| *block.hash())
        .collect();
        assert_eq!(expected_headers, actual_headers);
    }

    #[test]
    fn should_return_highest_block_header_from_trusted_block() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        // `sync_leap` is a well formed SyncLeap structure for the test chain. We can use the blocks
        // it contains to generate SyncLeap structures as required for the test, because we know the
        // heights of the blocks in the test chain as well as their sigs.
        let highest_block = sync_leap
            .signed_block_headers
            .last()
            .unwrap()
            .block_header
            .clone();
        let lowest_blocks: Vec<_> = sync_leap
            .trusted_ancestor_headers
            .iter()
            .take(2)
            .cloned()
            .collect();
        let middle_blocks: Vec<_> = sync_leap
            .signed_block_headers
            .iter()
            .take(2)
            .cloned()
            .collect();

        let highest_block_height = highest_block.height();
        let highest_block_hash = highest_block.block_hash();

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: highest_block.clone(),
            trusted_ancestor_headers: lowest_blocks,
            signed_block_headers: middle_blocks,
        };
        assert_eq!(
            sync_leap
                .highest_block_header_and_signatures()
                .0
                .block_hash(),
            highest_block.block_hash()
        );
        assert_eq!(sync_leap.highest_block_hash(), highest_block_hash);
        assert_eq!(sync_leap.highest_block_height(), highest_block_height);
    }

    #[test]
    fn should_return_highest_block_header_from_trusted_ancestors() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        // `sync_leap` is a well formed SyncLeap structure for the test chain. We can use the blocks
        // it contains to generate SyncLeap structures as required for the test, because we know the
        // heights of the blocks in the test chain as well as their sigs.
        let highest_block = sync_leap
            .signed_block_headers
            .last()
            .unwrap()
            .block_header
            .clone();
        let lowest_blocks: Vec<_> = sync_leap
            .trusted_ancestor_headers
            .iter()
            .take(2)
            .cloned()
            .collect();
        let middle_blocks: Vec<_> = sync_leap
            .signed_block_headers
            .iter()
            .take(2)
            .cloned()
            .collect();

        let highest_block_height = highest_block.height();
        let highest_block_hash = highest_block.block_hash();

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: lowest_blocks.first().unwrap().clone(),
            trusted_ancestor_headers: vec![highest_block],
            signed_block_headers: middle_blocks,
        };
        assert_eq!(
            sync_leap
                .highest_block_header_and_signatures()
                .0
                .block_hash(),
            highest_block_hash
        );
        assert_eq!(sync_leap.highest_block_hash(), highest_block_hash);
        assert_eq!(sync_leap.highest_block_height(), highest_block_height);
    }

    #[test]
    fn should_return_highest_block_header_from_signed_block_headers() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        // `sync_leap` is a well formed SyncLeap structure for the test chain. We can use the blocks
        // it contains to generate SyncLeap structures as required for the test, because we know the
        // heights of the blocks in the test chain as well as their sigs.
        let highest_block = sync_leap.signed_block_headers.last().unwrap().clone();
        let lowest_blocks: Vec<_> = sync_leap
            .trusted_ancestor_headers
            .iter()
            .take(2)
            .cloned()
            .collect();
        let middle_blocks: Vec<_> = sync_leap
            .signed_block_headers
            .iter()
            .take(2)
            .cloned()
            .map(|block_header_with_metadata| block_header_with_metadata.block_header)
            .collect();

        let highest_block_height = highest_block.block_header.height();
        let highest_block_hash = highest_block.block_header.block_hash();

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: lowest_blocks.first().unwrap().clone(),
            trusted_ancestor_headers: middle_blocks,
            signed_block_headers: vec![highest_block.clone()],
        };
        assert_eq!(
            sync_leap
                .highest_block_header_and_signatures()
                .0
                .block_hash(),
            highest_block.block_header.block_hash()
        );
        assert_eq!(sync_leap.highest_block_hash(), highest_block_hash);
        assert_eq!(sync_leap.highest_block_height(), highest_block_height);
    }

    #[test]
    fn should_return_sigs_when_highest_block_is_signed() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        // `sync_leap` is a well formed SyncLeap structure for the test chain. We can use the blocks
        // it contains to generate SyncLeap structures as required for the test, because we know the
        // heights of the blocks in the test chain as well as their sigs.
        let highest_block = sync_leap.signed_block_headers.last().unwrap().clone();
        let lowest_blocks: Vec<_> = sync_leap
            .trusted_ancestor_headers
            .iter()
            .take(2)
            .cloned()
            .collect();
        let middle_blocks: Vec<_> = sync_leap
            .signed_block_headers
            .iter()
            .take(2)
            .cloned()
            .map(|block_header_with_metadata| block_header_with_metadata.block_header)
            .collect();
        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: lowest_blocks.first().unwrap().clone(),
            trusted_ancestor_headers: middle_blocks,
            signed_block_headers: vec![highest_block],
        };
        assert!(sync_leap.highest_block_header_and_signatures().1.is_some());
    }

    #[test]
    fn should_not_return_sigs_when_highest_block_is_not_signed() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];

        let mut rng = TestRng::new();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let sync_leap = make_test_sync_leap(
            &mut rng,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        // `sync_leap` is a well formed SyncLeap structure for the test chain. We can use the blocks
        // it contains to generate SyncLeap structures as required for the test, because we know the
        // heights of the blocks in the test chain as well as their sigs.
        let highest_block = sync_leap.signed_block_headers.last().unwrap().clone();
        let lowest_blocks: Vec<_> = sync_leap
            .trusted_ancestor_headers
            .iter()
            .take(2)
            .cloned()
            .collect();
        let middle_blocks: Vec<_> = sync_leap
            .signed_block_headers
            .iter()
            .take(2)
            .cloned()
            .collect();
        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: highest_block.block_header,
            trusted_ancestor_headers: lowest_blocks,
            signed_block_headers: middle_blocks,
        };
        assert!(sync_leap.highest_block_header_and_signatures().1.is_none());
    }

    #[test]
    fn should_return_era_validator_weights_for_correct_sync_leap() {
        // Chain
        // 0   1   2   3   4   5   6   7   8   9   10   11
        // S           S           S           S
        let switch_blocks = [0, 3, 6, 9];

        let mut rng = TestRng::new();

        // Test block iterator will pull 2 validators for each created block. Indices 0 and 1 are
        // used for validators for the trusted ancestor headers.
        const FIRST_SIGNED_BLOCK_HEADER_VALIDATOR_OFFSET: usize = 2;
        let validators: Vec<_> = (1..100)
            .map(|weight| {
                let (secret_key, public_key) = crypto::generate_ed25519_keypair();
                ValidatorSpec {
                    secret_key,
                    public_key,
                    weight: Some(U512::from(weight)),
                }
            })
            .collect();

        let query = 5;
        let trusted_ancestor_headers = [4, 3];
        let signed_block_headers = [6, 9, 11];
        let add_proofs = true;
        let sync_leap = make_test_sync_leap_with_validators(
            &mut rng,
            &validators,
            &switch_blocks,
            query,
            &trusted_ancestor_headers,
            &signed_block_headers,
            add_proofs,
        );

        let fault_tolerance_fraction = Ratio::new_raw(1, 3);

        let mut switch_block_iter = sync_leap.signed_block_headers.iter();
        let first_switch_block = switch_block_iter.next().unwrap().clone();
        let validator_1 = validators
            .get(FIRST_SIGNED_BLOCK_HEADER_VALIDATOR_OFFSET)
            .unwrap();
        let validator_2 = validators
            .get(FIRST_SIGNED_BLOCK_HEADER_VALIDATOR_OFFSET + 1)
            .unwrap();
        let first_era_validator_weights = EraValidatorWeights::new(
            first_switch_block.block_header.era_id(),
            [validator_1, validator_2]
                .iter()
                .map(
                    |ValidatorSpec {
                         secret_key: _,
                         public_key,
                         weight,
                     }| (public_key.clone(), weight.unwrap()),
                )
                .collect(),
            fault_tolerance_fraction,
        );

        let second_switch_block = switch_block_iter.next().unwrap().clone();
        let validator_1 = validators
            .get(FIRST_SIGNED_BLOCK_HEADER_VALIDATOR_OFFSET + 2)
            .unwrap();
        let validator_2 = validators
            .get(FIRST_SIGNED_BLOCK_HEADER_VALIDATOR_OFFSET + 3)
            .unwrap();
        let second_era_validator_weights = EraValidatorWeights::new(
            second_switch_block.block_header.era_id(),
            [validator_1, validator_2]
                .iter()
                .map(
                    |ValidatorSpec {
                         secret_key: _,
                         public_key,
                         weight,
                     }| (public_key.clone(), weight.unwrap()),
                )
                .collect(),
            fault_tolerance_fraction,
        );

        let third_switch_block = switch_block_iter.next().unwrap().clone();
        let validator_1 = validators
            .get(FIRST_SIGNED_BLOCK_HEADER_VALIDATOR_OFFSET + 4)
            .unwrap();
        let validator_2 = validators
            .get(FIRST_SIGNED_BLOCK_HEADER_VALIDATOR_OFFSET + 5)
            .unwrap();
        let third_era_validator_weights = EraValidatorWeights::new(
            third_switch_block.block_header.era_id(),
            [validator_1, validator_2]
                .iter()
                .map(
                    |ValidatorSpec {
                         secret_key: _,
                         public_key,
                         weight,
                     }| (public_key.clone(), weight.unwrap()),
                )
                .collect(),
            fault_tolerance_fraction,
        );

        let result: Vec<_> = sync_leap
            .era_validator_weights(fault_tolerance_fraction)
            .collect();
        assert_eq!(
            result,
            vec![
                first_era_validator_weights,
                second_era_validator_weights,
                third_era_validator_weights
            ]
        )
    }

    #[test]
    fn era_validator_weights_without_genesis_without_switch_block_preceding_immediate_switch_block()
    {
        let mut rng = TestRng::new();

        let trusted_block = Block::random_non_switch_block(&mut rng);

        let (
            signed_block_header_with_metadata_1,
            signed_block_header_with_metadata_2,
            signed_block_header_with_metadata_3,
        ) = make_three_switch_blocks_at_era_and_height(rng, (1, 10), (2, 20), (3, 30));

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: trusted_block.header().clone(),
            trusted_ancestor_headers: vec![],
            signed_block_headers: vec![
                signed_block_header_with_metadata_1,
                signed_block_header_with_metadata_2,
                signed_block_header_with_metadata_3,
            ],
        };

        let fault_tolerance_fraction = Ratio::new_raw(1, 3);

        // Assert only if correct eras are selected, since the the
        // `should_return_era_validator_weights_for_correct_sync_leap` test already covers the
        // actual weight validation.

        let actual_eras: BTreeSet<u64> = sync_leap
            .era_validator_weights(fault_tolerance_fraction)
            .map(|era_validator_weights| era_validator_weights.era_id().into())
            .collect();
        let mut expected_eras: BTreeSet<u64> = BTreeSet::new();
        // Expect successors of the eras of switch blocks.
        expected_eras.extend([2, 3, 4]);
        assert_eq!(expected_eras, actual_eras);
    }

    #[test]
    fn era_validator_weights_without_genesis_with_switch_block_preceding_immediate_switch_block() {
        let mut rng = TestRng::new();

        let trusted_block = Block::random_non_switch_block(&mut rng);

        let (
            signed_block_header_with_metadata_1,
            signed_block_header_with_metadata_2,
            signed_block_header_with_metadata_3,
        ) = make_three_switch_blocks_at_era_and_height(rng, (1, 10), (2, 20), (3, 21));

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: trusted_block.header().clone(),
            trusted_ancestor_headers: vec![],
            signed_block_headers: vec![
                signed_block_header_with_metadata_1,
                signed_block_header_with_metadata_2,
                signed_block_header_with_metadata_3,
            ],
        };

        let fault_tolerance_fraction = Ratio::new_raw(1, 3);

        // Assert only if correct eras are selected, since the the
        // `should_return_era_validator_weights_for_correct_sync_leap` test already covers the
        // actual weight validation.

        let actual_eras: BTreeSet<u64> = sync_leap
            .era_validator_weights(fault_tolerance_fraction)
            .map(|era_validator_weights| era_validator_weights.era_id().into())
            .collect();
        let mut expected_eras: BTreeSet<u64> = BTreeSet::new();

        // Block #1 (era=1, height=10)
        // Block #2 (era=2, height=20) - block preceding immediate switch block
        // Block #3 (era=3, height=21) - immediate switch block.
        // Expect the successor of block #2 to be not present.
        expected_eras.extend([2, 4]);
        assert_eq!(expected_eras, actual_eras);
    }

    #[test]
    fn era_validator_weights_with_genesis_without_switch_block_preceding_immediate_switch_block() {
        let mut rng = TestRng::new();

        let trusted_block = Block::random_non_switch_block(&mut rng);

        let (
            signed_block_header_with_metadata_1,
            signed_block_header_with_metadata_2,
            signed_block_header_with_metadata_3,
        ) = make_three_switch_blocks_at_era_and_height(rng, (0, 0), (2, 20), (3, 30));

        let sync_leap = SyncLeap {
            trusted_ancestor_only: false,
            trusted_block_header: trusted_block.header().clone(),
            trusted_ancestor_headers: vec![],
            signed_block_headers: vec![
                signed_block_header_with_metadata_1,
                signed_block_header_with_metadata_2,
                signed_block_header_with_metadata_3,
            ],
        };

        let fault_tolerance_fraction = Ratio::new_raw(1, 3);

        // Assert only if correct eras are selected, since the the
        // `should_return_era_validator_weights_for_correct_sync_leap` test already covers the
        // actual weight validation.

        let actual_eras: BTreeSet<u64> = sync_leap
            .era_validator_weights(fault_tolerance_fraction)
            .map(|era_validator_weights| era_validator_weights.era_id().into())
            .collect();
        let mut expected_eras: BTreeSet<u64> = BTreeSet::new();
        // Expect genesis era id and its successor as well as the successors of the eras of
        // non-genesis switch blocks.
        expected_eras.extend([0, 1, 3, 4]);
        assert_eq!(expected_eras, actual_eras);
    }

    fn make_three_switch_blocks_at_era_and_height(
        mut rng: TestRng,
        (era_1, height_1): (u64, u64),
        (era_2, height_2): (u64, u64),
        (era_3, height_3): (u64, u64),
    ) -> (
        BlockHeaderWithMetadata,
        BlockHeaderWithMetadata,
        BlockHeaderWithMetadata,
    ) {
        let signed_block_1 =
            random_switch_block_at_height_and_era(&mut rng, height_1, era_1.into());
        let signed_block_2 =
            random_switch_block_at_height_and_era(&mut rng, height_2, era_2.into());
        let signed_block_3 =
            random_switch_block_at_height_and_era(&mut rng, height_3, era_3.into());

        let signed_block_header_with_metadata_1 =
            make_signed_block_header_from_header(signed_block_1.header(), &[], false);
        let signed_block_header_with_metadata_2 =
            make_signed_block_header_from_header(signed_block_2.header(), &[], false);
        let signed_block_header_with_metadata_3 =
            make_signed_block_header_from_header(signed_block_3.header(), &[], false);
        (
            signed_block_header_with_metadata_1,
            signed_block_header_with_metadata_2,
            signed_block_header_with_metadata_3,
        )
    }

    #[test]
    fn should_construct_proper_sync_leap_identifier() {
        let mut rng = TestRng::new();

        let sync_leap_identifier = SyncLeapIdentifier::sync_to_tip(BlockHash::random(&mut rng));
        assert!(!sync_leap_identifier.trusted_ancestor_only());

        let sync_leap_identifier =
            SyncLeapIdentifier::sync_to_historical(BlockHash::random(&mut rng));
        assert!(sync_leap_identifier.trusted_ancestor_only());
    }

    // Describes a single item from the set of validators that will be used for switch blocks
    // created by TestChainSpec.
    pub(crate) struct ValidatorSpec {
        pub(crate) secret_key: SecretKey,
        pub(crate) public_key: PublicKey,
        // If `None`, weight will be chosen randomly.
        pub(crate) weight: Option<U512>,
    }

    // Utility struct that can be turned into an iterator that generates
    // continuous and descending blocks (i.e. blocks that have consecutive height
    // and parent hashes are correctly set). The height of the first block
    // in a series is chosen randomly.
    //
    // Additionally, this struct allows to generate switch blocks at a specific location in the
    // chain, for example: Setting `switch_block_indices` to [1; 3] and generating 5 blocks will
    // cause the 2nd and 4th blocks to be switch blocks. Validators for all eras are filled from
    // the `validators` parameter.
    pub(crate) struct TestChainSpec<'a> {
        block: Block,
        rng: &'a mut TestRng,
        switch_block_indices: Option<Vec<u64>>,
        validators: &'a [ValidatorSpec],
    }

    impl<'a> TestChainSpec<'a> {
        pub(crate) fn new(
            test_rng: &'a mut TestRng,
            switch_block_indices: Option<Vec<u64>>,
            validators: &'a [ValidatorSpec],
        ) -> Self {
            let block = Block::random(test_rng);
            Self {
                block,
                rng: test_rng,
                switch_block_indices,
                validators,
            }
        }

        pub(crate) fn iter(&mut self) -> TestBlockIterator {
            let block_height = self.block.height();

            const DEFAULT_VALIDATOR_WEIGHT: u64 = 100;

            TestBlockIterator::new(
                self.block.clone(),
                self.rng,
                self.switch_block_indices
                    .clone()
                    .map(|switch_block_indices| {
                        switch_block_indices
                            .iter()
                            .map(|index| index + block_height)
                            .collect()
                    }),
                self.validators
                    .iter()
                    .map(
                        |ValidatorSpec {
                             secret_key: _,
                             public_key,
                             weight,
                         }| {
                            (
                                public_key.clone(),
                                weight.unwrap_or(DEFAULT_VALIDATOR_WEIGHT.into()),
                            )
                        },
                    )
                    .collect(),
            )
        }
    }

    pub(crate) struct TestBlockIterator<'a> {
        block: Block,
        rng: &'a mut TestRng,
        switch_block_indices: Option<Vec<u64>>,
        validators: Vec<(PublicKey, U512)>,
        next_validator_index: usize,
    }

    impl<'a> TestBlockIterator<'a> {
        pub fn new(
            block: Block,
            rng: &'a mut TestRng,
            switch_block_indices: Option<Vec<u64>>,
            validators: Vec<(PublicKey, U512)>,
        ) -> Self {
            Self {
                block,
                rng,
                switch_block_indices,
                validators,
                next_validator_index: 0,
            }
        }
    }

    impl<'a> Iterator for TestBlockIterator<'a> {
        type Item = Block;

        fn next(&mut self) -> Option<Self::Item> {
            let (is_switch_block, is_successor_of_switch_block, validators) =
                match &self.switch_block_indices {
                    Some(switch_block_heights)
                        if switch_block_heights.contains(&self.block.height()) =>
                    {
                        let is_successor_of_switch_block =
                            switch_block_heights.contains(&(self.block.height().saturating_sub(1)));
                        (
                            true,
                            is_successor_of_switch_block,
                            Some(self.validators.clone()),
                        )
                    }
                    Some(switch_block_heights) => {
                        let is_successor_of_switch_block =
                            switch_block_heights.contains(&(self.block.height().saturating_sub(1)));
                        (false, is_successor_of_switch_block, None)
                    }
                    None => (false, false, None),
                };

            let validators = if let Some(validators) = validators {
                let first_validator = validators.get(self.next_validator_index).unwrap();
                let second_validator = validators.get(self.next_validator_index + 1).unwrap();

                // Put two validators in each switch block.
                let mut validators_for_block = BTreeMap::new();
                validators_for_block.insert(first_validator.0.clone(), first_validator.1);
                validators_for_block.insert(second_validator.0.clone(), second_validator.1);
                self.next_validator_index += 2;

                // If we're out of validators, do round robin on the provided list.
                if self.next_validator_index >= self.validators.len() {
                    self.next_validator_index = 0;
                }
                Some(validators_for_block)
            } else {
                None
            };

            let next = Block::new(
                *self.block.hash(),
                self.block.header().accumulated_seed(),
                *self.block.header().state_root_hash(),
                FinalizedBlock::random_with_specifics(
                    self.rng,
                    if is_successor_of_switch_block {
                        self.block.header().era_id().successor()
                    } else {
                        self.block.header().era_id()
                    },
                    self.block.header().height() + 1,
                    is_switch_block,
                    Timestamp::now(),
                    iter::empty(),
                ),
                validators,
                self.block.header().protocol_version(),
            )
            .unwrap();
            self.block = next.clone();
            Some(next)
        }
    }

    #[test]
    fn should_create_valid_chain() {
        let mut rng = TestRng::new();
        let mut test_block = TestChainSpec::new(&mut rng, None, &[]);
        let mut block_batch = test_block.iter().take(100);
        let mut parent_block: Block = block_batch.next().unwrap();
        for current_block in block_batch {
            assert_eq!(
                current_block.header().height(),
                parent_block.header().height() + 1,
                "height should grow monotonically"
            );
            assert_eq!(
                current_block.header().parent_hash(),
                parent_block.hash(),
                "block's parent should point at previous block"
            );
            parent_block = current_block;
        }
    }

    #[test]
    fn should_create_switch_blocks() {
        let switch_block_indices = vec![0, 10, 76];

        let validators: Vec<_> = iter::repeat_with(crypto::generate_ed25519_keypair)
            .take(2)
            .map(|(secret_key, public_key)| ValidatorSpec {
                secret_key,
                public_key,
                weight: None,
            })
            .collect();

        let mut rng = TestRng::new();
        let mut test_block =
            TestChainSpec::new(&mut rng, Some(switch_block_indices.clone()), &validators);
        let block_batch: Vec<_> = test_block.iter().take(100).collect();

        let base_height = block_batch.first().expect("should have block").height();

        for block in block_batch {
            if switch_block_indices
                .iter()
                .map(|index| index + base_height)
                .any(|index| index == block.height())
            {
                assert!(block.header().is_switch_block())
            } else {
                assert!(!block.header().is_switch_block())
            }
        }
    }
}
