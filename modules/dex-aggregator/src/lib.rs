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

//! # Dex-aggregator Module
//!
//! ## Overview
//!
//! Allows Users to input tokens to swap and executes the cheapest path for that pair

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]

use frame_support::{pallet_prelude::*, transactional};
use frame_system::pallet_prelude::*;
use primitives::{Balance, CurrencyId, TradingPair};
use sp_std::vec;
use support::{AggregatorSuper, AvailablePool};

mod mock;
mod tests;

pub use module::*;

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Max lenghth of trading path
		#[pallet::constant]
		type AggregatorTradingPathLimit: Get<u32>;

		type Aggregator: AggregatorSuper<Self::AccountId, TradingPair, Balance>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	#[pallet::metadata(T::AccountId = "AccountId")]
	pub enum Event<T: Config> {
		/// Use supply currency to swap target currency. \[trader, trading_pair,
		/// supply_currency_amount, target_currency_amount\]
		Swap(T::AccountId, TradingPair, Balance, Balance),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Minimum target was higher than any possible path expected target output
		BelowMinimumTarget,
		/// Aggregator could not find any viable path to perform the swap
		NoPossibleTradingPath,
		/// Invalid CurrencyId
		InvalidCurrencyId,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Trading with DEX-Aggregator, swap with exact supply amount
		///
		/// - `supply_token`: CurrencyId of token input by user in swap
		/// - `target_token`: CurrencyId of token recieved by user in swap
		/// - `supply_amount`: exact supply amount.
		/// - `min_target_amount`: acceptable minimum target amount.
		#[pallet::weight(10000)]
		#[transactional]
		pub fn swap_with_exact_supply(
			origin: OriginFor<T>,
			supply_token: CurrencyId,
			target_token: CurrencyId,
			#[pallet::compact] supply_amount: Balance,
			#[pallet::compact] min_target_amount: Balance,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let pair = TradingPair::from_currency_ids_unordered(supply_token, target_token)
				.ok_or(Error::<T>::InvalidCurrencyId)?;

			let best_path =
				Self::optimal_path_with_exact_supply(pair, supply_amount).ok_or(Error::<T>::NoPossibleTradingPath)?;
			ensure!(best_path.1 > min_target_amount, Error::<T>::BelowMinimumTarget);
			let mut balance = supply_amount;

			let last_path_elem = best_path.0.len() - 1;

			for (i, pool) in best_path.0.into_iter().enumerate() {
				if i == last_path_elem {
					// last element uses slippage tolerance of min_target amount
					balance = Self::do_swap_with_exact_supply(&who, &pool, balance, min_target_amount)?;
				} else {
					// all pools that are not the final swap execute regardless of slippage... the transactional
					// attribute should revert any state changes if the end of the chain of swaps results in a target
					// amount < min target amount
					balance = Self::do_swap_with_exact_supply(&who, &pool, balance, 0)?;
				}
			}
			Self::deposit_event(Event::Swap(who, pair, supply_amount, balance));
			Ok(())
		}

		/*
				/// Trading with DEX-Aggregator, swap with exact supply amount
				///
				/// - `supply_token`: CurrencyId of token input by user in swap
				/// - `target_token`: CurrencyId of token recieved by user in swap
				/// - `target_amount`: exact target amount.
				/// - `max_supply_amount`: acceptable maximum supply amount.
				#[pallet::weight(10000)]
				#[transactional]
				pub fn swap_with_exact_target(
					origin: OriginFor<T>,
					supply_token: CurrencyId,
					target_token: CurrencyId,
					#[pallet::compact] target_amount: Balance,
					#[pallet::compact] max_supply_amount: Balance,
				) -> DispatchResult {
					let who = ensure_signed(origin)?;
					Ok(())
				}
		*/
	}
}

impl<T: Config> Pallet<T> {
	/// Retrieves all available pools that can perform swaps of trading pairs
	fn all_active_pairs() -> Vec<AvailablePool> {
		T::Aggregator::all_active_pairs()
	}

	/// Returns supply amount given the trading path and the target amount. Returns None if path
	/// cannot be swapped.
	fn get_supply_amount(path: Vec<AvailablePool>, target_amount: Balance) -> Option<Balance> {
		let mut cache_money = target_amount;
		// CurrencyId of final target currency
		let mut cache_pool: CurrencyId = match path.len() {
			0 => return None,
			n => path[n - 1].1.second(),
		};

		// Iterates through the trading path starting at the final step and gets the supply amount for each
		// AvailablePool this is then used as the target amount for the next iteration
		for pool in path.iter().rev() {
			if cache_pool == pool.1.clone().second() {
				cache_money = match T::Aggregator::pallet_get_supply_amount(*pool, cache_money) {
					Some(i) => i,
					None => return None,
				};
				cache_pool = pool.1.clone().first();
			} else {
				return None;
			}
		}
		Some(cache_money)
	}

	/// Returns target amount given the trading path and the supply amount. Returns None if path
	/// cannot be swapped.
	fn get_target_amount(path: Vec<AvailablePool>, supply_amount: Balance) -> Option<Balance> {
		let mut cache_money = supply_amount;
		if path.is_empty() {
			return None;
		}
		// Can panic but above line checks if vec is empty
		// CurrencyId of supply currency
		let mut cache_pool: CurrencyId = path[0].1.first();

		// Iterates through the trading path and gets the target amount for each AvailablePool given a
		// supply which then is used as the supply for the next iteration
		for pool in path.iter() {
			if cache_pool == pool.1.clone().first() {
				cache_money = match T::Aggregator::pallet_get_target_amount(*pool, cache_money) {
					Some(i) => i,
					None => return None,
				};
				cache_pool = pool.1.clone().second()
			} else {
				return None;
			}
		}
		Some(cache_money)
	}

	/// Returns tuple of optimal path with expected target amount. Returns None if trade is not
	/// possible TODO: This is an very ugly placeholder algorithm to get a working model for initial
	/// review, must build algorithm for length of n, as well as optimize and beautify. related: https://stackoverflow.com/questions/12293870/algorithm-to-get-all-possible-string-combinations-from-array-up-to-certain-lengt
	/// https://stackoverflow.com/questions/361/generate-list-of-all-possible-permutations-of-a-string
	fn optimal_path_with_exact_supply(
		pair: TradingPair,
		supply_amount: Balance,
	) -> Option<(Vec<AvailablePool>, Balance)> {
		let mut i: u32 = 0;
		let all_pools = Self::all_active_pairs();
		let mut optimal_path: Vec<AvailablePool> = Vec::new();
		let mut optimal_balance: Balance = 0;
		while i < T::AggregatorTradingPathLimit::get() {
			if i == 0 {
				for pool in all_pools.clone() {
					if pair == pool.1 {
						if let Some(new_balance) = Self::get_target_amount(vec![pool], supply_amount) {
							if new_balance > optimal_balance {
								optimal_balance = new_balance;
								optimal_path = vec![pool];
							}
						}
					}

					if pair == pool.1.swap() {
						if let Some(new_balance) = Self::get_target_amount(vec![pool.swap()], supply_amount) {
							if new_balance > optimal_balance {
								optimal_balance = new_balance;
								optimal_path = vec![pool.swap()];
							}
						}
					}
				}
			} else if i == 1 {
				let mut possible_paths = Vec::new();
				for pool in all_pools.clone() {
					if pair.first() == pool.1.first() {
						possible_paths.push(pool);
					} else if pair.first() == pool.1.second() {
						possible_paths.push(pool.swap())
					}
					for pool1 in possible_paths.clone() {
						for pool2 in all_pools.clone() {
							if pair.second() == pool2.1.second() && pool1.1.second() == pool2.1.first() {
								if let Some(new_balance) = Self::get_target_amount(vec![pool1, pool2], supply_amount) {
									if new_balance > optimal_balance {
										optimal_balance = new_balance;
										optimal_path = vec![pool1, pool2];
									}
								}
							} else if pair.second() == pool2.1.first() && pool1.1.second() == pool2.1.second() {
								if let Some(new_balance) =
									Self::get_target_amount(vec![pool1, pool2.swap()], supply_amount)
								{
									if new_balance > optimal_balance {
										optimal_balance = new_balance;
										optimal_path = vec![pool1, pool2.swap()];
									}
								}
							}
						}
					}
				}
			}
			i += 1;
		}

		if optimal_path.is_empty() {
			None
		} else {
			Some((optimal_path, optimal_balance))
		}
	}

	fn do_swap_with_exact_supply(
		who: &T::AccountId,
		pool: &AvailablePool,
		supply_amount: Balance,
		min_target_amount: Balance,
	) -> sp_std::result::Result<Balance, DispatchError> {
		T::Aggregator::aggregator_swap_with_exact_supply(who, pool, supply_amount, min_target_amount)
	}

	fn do_swap_with_exact_target(
		who: &T::AccountId,
		pool: &AvailablePool,
		target_amount: Balance,
		max_supply_amount: Balance,
	) -> sp_std::result::Result<Balance, DispatchError> {
		T::Aggregator::aggregator_swap_with_exact_target(who, pool, target_amount, max_supply_amount)
	}
}