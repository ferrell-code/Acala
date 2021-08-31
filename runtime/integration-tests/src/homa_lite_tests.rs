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

//! Tests the Homa-lite module, and its cross-chain functionalities.

#[cfg(any(feature = "with-mandala-runtime", feature = "with-karura-runtime"))]
mod common_tests {
	use crate::integration_tests::*;
	use frame_support::{assert_noop, assert_ok};
	use orml_traits::MultiCurrency;

	#[test]
	fn homa_lite_mint_works() {
		ExtBuilder::default()
			.balances(vec![
				(alice(), RELAY_CHAIN_CURRENCY, 5_000 * dollar(RELAY_CHAIN_CURRENCY)),
				(bob(), RELAY_CHAIN_CURRENCY, 5_000 * dollar(RELAY_CHAIN_CURRENCY)),
				(bob(), LIQUID_CURRENCY, 1_000_000 * dollar(LIQUID_CURRENCY)),
			])
			.build()
			.execute_with(|| {
				let amount = 1000 * dollar(RELAY_CHAIN_CURRENCY);

				assert_noop!(
					HomaLite::mint(Origin::signed(alice()), amount),
					module_homa_lite::Error::<Runtime>::ExceededStakingCurrencyMintCap
				);

				// Set the total staking amount
				let liquid_issuance = Currencies::total_issuance(LIQUID_CURRENCY);
				assert_eq!(liquid_issuance, 1_000_000 * dollar(LIQUID_CURRENCY));

				let staking_total = liquid_issuance / 5;

				// Set the exchange rate to 1(S) : 5(L)
				assert_ok!(HomaLite::set_total_staking_currency(Origin::root(), staking_total));

				assert_ok!(HomaLite::set_minting_cap(Origin::root(), 10 * staking_total));

				// MaxRewardPerEra = 0.0005
				// MintFee: Balance = 0.0002

				// Exchange rate set to 1(Staking) : 5(Liquid) ratio
				// liquid = (amount - MintFee) * exchange_rate * (1 - MaxRewardPerEra)
				//        = (1000 - 0.0002)  * 5 * 0.9995 = 4997.4990005
				let liquid_amount_1 = 49_974_990_005 * dollar(RELAY_CHAIN_CURRENCY) / 10_000_000;

				assert_ok!(HomaLite::mint(Origin::signed(alice()), amount));
				assert_eq!(Currencies::free_balance(LIQUID_CURRENCY, &alice()), liquid_amount_1);
				System::assert_last_event(Event::HomaLite(module_homa_lite::Event::Minted(
					alice(),
					amount,
					liquid_amount_1,
				)));

				// Total issuance for liquid currnecy increased.
				let new_liquid_issuance = Currencies::total_issuance(LIQUID_CURRENCY);
				#[cfg(feature = "with-mandala-runtime")]
				assert_eq!(new_liquid_issuance, 10_049_974_990_005_000);
				#[cfg(feature = "with-karura-runtime")]
				assert_eq!(new_liquid_issuance, 1_004_997_499_000_500_000);

				// liquid = (amount - MintFee) * (new_liquid_issuance / new_staking_total) * (1 - MaxRewardPerEra)
				//        = (1000 - 0.0002) * (1004997.4990005 / 201000) * 0.9995 = 4997.486563940297
				#[cfg(feature = "with-mandala-runtime")] // Mandala uses DOT, which has 10 d.p. accuracy.
				let liquid_amount_2 = 49_974_865_639_403;
				#[cfg(feature = "with-karura-runtime")] // Karura uses KSM, which has 12 d.p. accuracy.
				let liquid_amount_2 = 4_997_486_563_940_297;

				assert_ok!(HomaLite::mint(Origin::signed(alice()), amount));
				System::assert_last_event(Event::HomaLite(module_homa_lite::Event::Minted(
					alice(),
					amount,
					liquid_amount_2,
				)));

				#[cfg(feature = "with-mandala-runtime")]
				assert_eq!(Currencies::free_balance(LIQUID_CURRENCY, &alice()), 99_949_855_644_403);
				#[cfg(feature = "with-karura-runtime")]
				assert_eq!(
					Currencies::free_balance(LIQUID_CURRENCY, &alice()),
					9_994_985_564_440_297
				);
			});
	}
}

#[cfg(feature = "with-karura-runtime")]
mod karura_only_tests {
	use crate::integration_tests::*;
	use crate::kusama_test_net::*;

	use frame_support::assert_ok;
	use orml_traits::MultiCurrency;
	use sp_runtime::MultiAddress;

	use xcm::v0::{
		Junction::{self, Parachain},
		MultiAsset::*,
		MultiLocation::*,
	};
	use xcm_emulator::TestExt;

	#[test]
	fn homa_lite_xcm_transfer() {
		let homa_lite_sub_account: AccountId =
			hex_literal::hex!["d7b8926b326dd349355a9a7cca6606c1e0eb6fd2b506066b518c7155ff0d8297"].into();
		Kusama::execute_with(|| {
			// Transfer some KSM into the parachain.
			assert_ok!(kusama_runtime::XcmPallet::reserve_transfer_assets(
				kusama_runtime::Origin::signed(ALICE.into()),
				X1(Parachain(2000)),
				X1(Junction::AccountId32 {
					id: alice().into(),
					network: NetworkId::Any
				}),
				vec![ConcreteFungible {
					id: Null,
					amount: 2001 * dollar(KSM)
				}],
				600_000_000
			));

			// This account starts off with no fund.
			assert_eq!(kusama_runtime::Balances::free_balance(&homa_lite_sub_account), 0);
		});

		Karura::execute_with(|| {
			assert_ok!(Tokens::set_balance(
				Origin::root(),
				MultiAddress::Id(AccountId::from(bob())),
				LIQUID_CURRENCY,
				1_000_000 * dollar(LIQUID_CURRENCY),
				0
			));

			let amount = 1000 * dollar(RELAY_CHAIN_CURRENCY);

			// Set the total staking amount
			let liquid_issuance = Currencies::total_issuance(LIQUID_CURRENCY);
			assert_eq!(liquid_issuance, 1_000_000 * dollar(LIQUID_CURRENCY));

			let staking_total = 200_000 * dollar(LIQUID_CURRENCY);

			// Set the exchange rate to 1(S) : 5(L)
			assert_ok!(HomaLite::set_total_staking_currency(Origin::root(), staking_total));
			assert_ok!(HomaLite::set_xcm_dest_weight(Origin::root(), 1_000_000_000_000));

			assert_ok!(HomaLite::set_minting_cap(Origin::root(), 10 * staking_total));

			// Perform 2 mint actions, each 1000 dollars.
			assert_ok!(HomaLite::mint(Origin::signed(alice()), amount));
			assert_ok!(HomaLite::mint(Origin::signed(alice()), amount));

			// Most balances transferred into Kusama. Some extra fee is deducted as gas
			assert_eq!(Tokens::free_balance(RELAY_CHAIN_CURRENCY, &alice()), 999_952_000_000);
		});

		Kusama::execute_with(|| {
			// Check of 2000 dollars (minus some fee) are transferred into the Kusama chain.
			assert_eq!(
				kusama_runtime::Balances::free_balance(&homa_lite_sub_account),
				1_999_946_666_670_000
			);
		});
	}
}