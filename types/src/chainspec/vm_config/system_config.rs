#[cfg(feature = "datasize")]
use datasize::DataSize;
#[cfg(any(feature = "testing", test))]
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use serde::{Deserialize, Serialize};

use crate::{
    bytesrepr::{self, FromBytes, ToBytes},
    chainspec::vm_config::{AuctionCosts, HandlePaymentCosts, MintCosts, StandardPaymentCosts},
};

/// Default gas cost for a wasmless transfer.
pub const DEFAULT_WASMLESS_TRANSFER_COST: u32 = 0; // 100_000_000; TODO: reinstate when adding new payment logic

/// Default gas cost for an install/upgrader.
pub const DEFAULT_INSTALL_UPGRADE_COST: u64 = 500_000_000_000;

/// Default gas cost for a standard transaction.
pub const DEFAULT_STANDARD_TRANSACTION_COST: u64 = 50_000_000_000;


/// Definition of costs in the system.
///
/// This structure contains the costs of all the system contract's entry points and, additionally,
/// it defines a wasmless transfer cost.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "datasize", derive(DataSize))]
#[serde(deny_unknown_fields)]
pub struct SystemConfig {
    /// Wasmless transfer cost expressed in gas.
    wasmless_transfer_cost: u32,

    /// Install/Upgrader cost expressed in gas.
    install_upgrade_cost: u64,

    /// Standard transaction expressed in gas.
    standard_cost: u64,

    /// Configuration of auction entrypoint costs.
    auction_costs: AuctionCosts,

    /// Configuration of mint entrypoint costs.
    mint_costs: MintCosts,

    /// Configuration of handle payment entrypoint costs.
    handle_payment_costs: HandlePaymentCosts,

    /// Configuration of standard payment costs.
    standard_payment_costs: StandardPaymentCosts,
}

impl SystemConfig {
    /// Creates new system config instance.
    pub fn new(
        wasmless_transfer_cost: u32,
        install_upgrade_cost: u64,
        standard_cost: u64,
        auction_costs: AuctionCosts,
        mint_costs: MintCosts,
        handle_payment_costs: HandlePaymentCosts,
        standard_payment_costs: StandardPaymentCosts,
    ) -> Self {
        Self {
            wasmless_transfer_cost,
            install_upgrade_cost,
            standard_cost,
            auction_costs,
            mint_costs,
            handle_payment_costs,
            standard_payment_costs,
        }
    }

    /// Returns wasmless transfer cost.
    pub fn wasmless_transfer_cost(&self) -> u32 {
        self.wasmless_transfer_cost
    }

    /// Returns the install/upgrade cost.
    pub fn install_upgrade_cost(&self) -> u64 {
        self.install_upgrade_cost
    }

    /// Returns the standard transaction cost.
    pub fn standard_transaction_cost(&self) -> u64 {
        self.standard_cost
    }

    /// Returns the costs of executing auction entry points.
    pub fn auction_costs(&self) -> &AuctionCosts {
        &self.auction_costs
    }

    /// Returns the costs of executing mint entry points.
    pub fn mint_costs(&self) -> &MintCosts {
        &self.mint_costs
    }

    /// Returns the costs of executing `handle_payment` entry points.
    pub fn handle_payment_costs(&self) -> &HandlePaymentCosts {
        &self.handle_payment_costs
    }

    /// Returns the costs of executing `standard_payment` entry points.
    pub fn standard_payment_costs(&self) -> &StandardPaymentCosts {
        &self.standard_payment_costs
    }
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            wasmless_transfer_cost: DEFAULT_WASMLESS_TRANSFER_COST,
            install_upgrade_cost: DEFAULT_INSTALL_UPGRADE_COST,
            standard_cost: DEFAULT_STANDARD_TRANSACTION_COST,
            auction_costs: AuctionCosts::default(),
            mint_costs: MintCosts::default(),
            handle_payment_costs: HandlePaymentCosts::default(),
            standard_payment_costs: StandardPaymentCosts::default(),
        }
    }
}

#[cfg(any(feature = "testing", test))]
impl Distribution<SystemConfig> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> SystemConfig {
        SystemConfig {
            wasmless_transfer_cost: rng.gen(),
            install_upgrade_cost: rng.gen(),
            standard_cost: rng.gen(),
            auction_costs: rng.gen(),
            mint_costs: rng.gen(),
            handle_payment_costs: rng.gen(),
            standard_payment_costs: rng.gen(),
        }
    }
}

impl ToBytes for SystemConfig {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut ret = bytesrepr::unchecked_allocate_buffer(self);

        ret.append(&mut self.wasmless_transfer_cost.to_bytes()?);
        ret.append(&mut self.install_upgrade_cost.to_bytes()?);
        ret.append(&mut self.standard_cost.to_bytes()?);
        ret.append(&mut self.auction_costs.to_bytes()?);
        ret.append(&mut self.mint_costs.to_bytes()?);
        ret.append(&mut self.handle_payment_costs.to_bytes()?);
        ret.append(&mut self.standard_payment_costs.to_bytes()?);

        Ok(ret)
    }

    fn serialized_length(&self) -> usize {
        self.wasmless_transfer_cost.serialized_length()
            + self.install_upgrade_cost.serialized_length()
            + self.standard_cost.serialized_length()
            + self.auction_costs.serialized_length()
            + self.mint_costs.serialized_length()
            + self.handle_payment_costs.serialized_length()
            + self.standard_payment_costs.serialized_length()
    }
}

impl FromBytes for SystemConfig {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (wasmless_transfer_cost, rem) = FromBytes::from_bytes(bytes)?;
        let (install_upgrade_cost, rem) = FromBytes::from_bytes(rem)?;
        let (standard_cost, rem) = FromBytes::from_bytes(rem)?;
        let (auction_costs, rem) = FromBytes::from_bytes(rem)?;
        let (mint_costs, rem) = FromBytes::from_bytes(rem)?;
        let (handle_payment_costs, rem) = FromBytes::from_bytes(rem)?;
        let (standard_payment_costs, rem) = FromBytes::from_bytes(rem)?;
        Ok((
            SystemConfig::new(
                wasmless_transfer_cost,
                install_upgrade_cost,
                standard_cost,
                auction_costs,
                mint_costs,
                handle_payment_costs,
                standard_payment_costs,
            ),
            rem,
        ))
    }
}

#[doc(hidden)]
#[cfg(any(feature = "gens", test))]
pub mod gens {
    use proptest::{num, prop_compose};

    use crate::{
        chainspec::vm_config::{
            auction_costs::gens::auction_costs_arb,
            handle_payment_costs::gens::handle_payment_costs_arb, mint_costs::gens::mint_costs_arb,
            standard_payment_costs::gens::standard_payment_costs_arb,
        },
        SystemConfig,
    };

    prop_compose! {
        pub fn system_config_arb()(
            wasmless_transfer_cost in num::u32::ANY,
            install_upgrade_cost in num::u64::ANY,
            standard_cost in num::u64::ANY,
            auction_costs in auction_costs_arb(),
            mint_costs in mint_costs_arb(),
            handle_payment_costs in handle_payment_costs_arb(),
            standard_payment_costs in standard_payment_costs_arb(),
        ) -> SystemConfig {
            SystemConfig {
                wasmless_transfer_cost,
                install_upgrade_cost,
                standard_cost,
                auction_costs,
                mint_costs,
                handle_payment_costs,
                standard_payment_costs,
            }
        }
    }
}
