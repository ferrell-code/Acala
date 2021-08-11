// This file is part of Acala.

// Copyright (C) 2020-2021 Acala Foundation.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Mocks for the dex-aggregator module.

#![cfg(test)]

use super::*;
use frame_support::{construct_runtime, ord_parameter_types, parameter_types, PalletId};
use frame_system::EnsureSignedBy;
use orml_traits::{parameter_type_with_key, MultiReservableCurrency};
use primitives::{Amount, TokenSymbol};
use sp_core::H256;
use sp_runtime::{testing::Header, traits::IdentityLookup};
use support::{AggregatorManager, AggregatorSuper, AvailableAmm, AvailablePool};

pub type BlockNumber = u64;
pub type AccountId = u128;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const AUSD: CurrencyId = CurrencyId::Token(TokenSymbol::AUSD);
pub const BTC: CurrencyId = CurrencyId::Token(TokenSymbol::RENBTC);
pub const DOT: CurrencyId = CurrencyId::Token(TokenSymbol::DOT);
pub const ACA: CurrencyId = CurrencyId::Token(TokenSymbol::ACA);

parameter_types! {
	pub static AUSDBTCPair: TradingPair = TradingPair::from_currency_ids(AUSD, BTC).unwrap();
	pub static AUSDDOTPair: TradingPair = TradingPair::from_currency_ids(AUSD, DOT).unwrap();
	pub static DOTBTCPair: TradingPair = TradingPair::from_currency_ids(DOT, BTC).unwrap();
}

mod dex_aggregator {
	pub use super::super::*;
}

parameter_types! {
	pub const BlockHashCount: BlockNumber = 250;
}

impl frame_system::Config for Runtime {
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = BlockNumber;
	type Call = Call;
	type Hash = H256;
	type Hashing = ::sp_runtime::traits::BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type BlockWeights = ();
	type BlockLength = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type DbWeight = ();
	type BaseCallFilter = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
}

parameter_type_with_key! {
	pub ExistentialDeposits: |_currency_id: CurrencyId| -> Balance {
		Default::default()
	};
}

impl orml_tokens::Config for Runtime {
	type Event = Event;
	type Balance = Balance;
	type Amount = Amount;
	type CurrencyId = CurrencyId;
	type WeightInfo = ();
	type ExistentialDeposits = ExistentialDeposits;
	type OnDust = ();
	type MaxLocks = ();
}

pub struct MockDEXIncentives;
impl support::DEXIncentives<AccountId, CurrencyId, Balance> for MockDEXIncentives {
	fn do_deposit_dex_share(who: &AccountId, lp_currency_id: CurrencyId, amount: Balance) -> DispatchResult {
		Tokens::reserve(lp_currency_id, who, amount)
	}

	fn do_withdraw_dex_share(who: &AccountId, lp_currency_id: CurrencyId, amount: Balance) -> DispatchResult {
		let _ = Tokens::unreserve(lp_currency_id, who, amount);
		Ok(())
	}
}

ord_parameter_types! {
	pub const ListingOrigin: AccountId = 3;
}

parameter_types! {
	pub const GetExchangeFee: (u32, u32) = (1, 100);
	pub const TradingPathLimit: u32 = 3;
	pub const DEXPalletId: PalletId = PalletId(*b"aca/dexm");
}

impl dex::Config for Runtime {
	type Event = Event;
	type Currency = Tokens;
	type GetExchangeFee = GetExchangeFee;
	type TradingPathLimit = TradingPathLimit;
	type PalletId = DEXPalletId;
	type CurrencyIdMapping = ();
	type WeightInfo = ();
	type DEXIncentives = MockDEXIncentives;
	type ListingOrigin = EnsureSignedBy<ListingOrigin, AccountId>;
}

pub struct MockAggregator;
impl AggregatorSuper<AccountId, TradingPair, Balance> for MockAggregator {
	fn all_active_pairs() -> Vec<AvailablePool> {
		dex::Pallet::<Runtime>::get_active_pools()
	}

	fn pallet_get_supply_amount(pool: AvailablePool, target_amount: Balance) -> Option<Balance> {
		match pool.0 {
			AvailableAmm::Dex => dex::Pallet::<Runtime>::aggregator_supply_amount(pool.1, target_amount),
			_ => None,
		}
	}

	fn pallet_get_target_amount(pool: AvailablePool, supply_amount: Balance) -> Option<Balance> {
		match pool.0 {
			AvailableAmm::Dex => dex::Pallet::<Runtime>::aggregator_target_amount(pool.1, supply_amount),
			_ => None,
		}
	}

	fn aggregator_swap_with_exact_supply(
		who: &AccountId,
		pool: &AvailablePool,
		supply_amount: Balance,
		min_target_amount: Balance,
	) -> sp_std::result::Result<Balance, DispatchError> {
		match pool.0 {
			AvailableAmm::Dex => {
				dex::Pallet::<Runtime>::aggregator_swap_with_exact_supply(who, pool, supply_amount, min_target_amount)
			}
			// defensively returns error. should not reach here
			_ => Err(DispatchError::Other(
				"Unexpected Pallet called in runtime for dex-aggregator, should not happen",
			)),
		}
	}

	fn aggregator_swap_with_exact_target(
		who: &AccountId,
		pool: &AvailablePool,
		target_amount: Balance,
		max_supply_amount: Balance,
	) -> sp_std::result::Result<Balance, DispatchError> {
		match pool.0 {
			AvailableAmm::Dex => {
				dex::Pallet::<Runtime>::aggregator_swap_with_exact_target(who, pool, target_amount, max_supply_amount)
			}
			_ => Err(DispatchError::Other(
				"Unexpected Pallet called in runtime for dex-aggregator, should not happen",
			)),
		}
	}
}

impl Config for Runtime {
	type Event = Event;
	type AggregatorTradingPathLimit = TradingPathLimit;
	type Aggregator = MockAggregator;
}

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Runtime>;
type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Storage, Config, Event<T>},
		DexModule: dex::{Pallet, Storage, Call, Event<T>, Config<T>},
		Tokens: orml_tokens::{Pallet, Storage, Event<T>, Config<T>},
		DexAggregator: dex_aggregator::{Pallet, Call, Event<T>}
	}
);

pub struct ExtBuilder {
	balances: Vec<(AccountId, CurrencyId, Balance)>,
	initial_listing_trading_pairs: Vec<(TradingPair, (Balance, Balance), (Balance, Balance), BlockNumber)>,
	initial_enabled_trading_pairs: Vec<TradingPair>,
	initial_added_liquidity_pools: Vec<(AccountId, Vec<(TradingPair, (Balance, Balance))>)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			balances: vec![
				(ALICE, AUSD, 1_000_000_000_000_000_000u128),
				(BOB, AUSD, 1_000_000_000_000_000_000u128),
				(ALICE, BTC, 1_000_000_000_000_000_000u128),
				(BOB, BTC, 1_000_000_000_000_000_000u128),
				(ALICE, DOT, 1_000_000_000_000_000_000u128),
				(BOB, DOT, 1_000_000_000_000_000_000u128),
			],
			initial_listing_trading_pairs: vec![],
			initial_enabled_trading_pairs: vec![],
			initial_added_liquidity_pools: vec![],
		}
	}
}

impl ExtBuilder {
	pub fn initialize_enabled_trading_pairs(mut self) -> Self {
		self.initial_enabled_trading_pairs = vec![AUSDDOTPair::get(), AUSDBTCPair::get(), DOTBTCPair::get()];
		self
	}

	pub fn initialize_added_liquidity_pools(mut self, who: AccountId) -> Self {
		self.initial_added_liquidity_pools = vec![(
			who,
			vec![
				(AUSDDOTPair::get(), (1_000_000u128, 2_000_000u128)),
				(AUSDBTCPair::get(), (1_000_000u128, 2_000_000u128)),
				(DOTBTCPair::get(), (1_000_000u128, 2_000_000u128)),
			],
		)];
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::default()
			.build_storage::<Runtime>()
			.unwrap();

		orml_tokens::GenesisConfig::<Runtime> {
			balances: self.balances,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		dex::GenesisConfig::<Runtime> {
			initial_listing_trading_pairs: self.initial_listing_trading_pairs,
			initial_enabled_trading_pairs: self.initial_enabled_trading_pairs,
			initial_added_liquidity_pools: self.initial_added_liquidity_pools,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		t.into()
	}
}