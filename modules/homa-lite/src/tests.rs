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

//! Unit tests for the Homa-Lite Module

#![cfg(test)]

use super::*;
use frame_support::{assert_noop, assert_ok};
use mock::{
	dollar, CDPEngine, Currencies, Event, ExtBuilder, HomaLite, Loans, MockPriceSource, Origin, Runtime, System, ACALA,
	ALICE, BOB, INITIAL_BALANCE, INVALID_CALLER, KSM, LKSM, ROOT,
};
use module_support::{Position, Price, Rate};
use orml_traits::Change;
use sp_runtime::traits::{BadOrigin, One};

#[test]
fn mock_initialize_token_works() {
	ExtBuilder::default().build().execute_with(|| {
		let initial_dollar = dollar(INITIAL_BALANCE);
		assert_eq!(Currencies::free_balance(KSM, &ALICE), initial_dollar);
		assert_eq!(Currencies::free_balance(KSM, &BOB), initial_dollar);
		assert_eq!(Currencies::free_balance(LKSM, &ROOT), initial_dollar);
		assert_eq!(Currencies::free_balance(ACALA, &ALICE), initial_dollar);
		assert_eq!(Currencies::free_balance(ACALA, &BOB), initial_dollar);
		assert_eq!(Currencies::free_balance(ACALA, &ROOT), initial_dollar);
	});
}

#[test]
fn mint_works() {
	ExtBuilder::default().build().execute_with(|| {
		let amount = dollar(1000);

		assert_ok!(HomaLite::set_minting_cap(
			Origin::signed(ROOT),
			5 * dollar(INITIAL_BALANCE)
		));

		assert_noop!(
			HomaLite::mint(Origin::signed(ROOT), amount),
			orml_tokens::Error::<Runtime>::BalanceTooLow
		);

		// Since the exchange rate is not set, use the default 1:10 ratio
		// liquid = (amount - MintFee) * 10 * (1 - MaxRewardPerEra)
		//        = 0.99 * (1000 - 0.01)  * 10 = 9899.901
		let mut liquid = 9_899_901_000_000_000;
		assert_ok!(HomaLite::mint(Origin::signed(ALICE), amount));
		assert_eq!(Currencies::free_balance(LKSM, &ALICE), liquid);
		assert_eq!(
			System::events().iter().last().unwrap().event,
			Event::HomaLite(crate::Event::Minted(ALICE, amount, liquid))
		);
		// The total staking currency is now increased.
		assert_eq!(TotalStakingCurrency::<Runtime>::get(), dollar(1000));

		// Set the total staking amount
		let lksm_issuance = Currencies::total_issuance(LKSM);
		assert_eq!(lksm_issuance, 1_009_899_901_000_000_000);

		// Set the exchange rate to 1(S) : 5(L)
		assert_ok!(HomaLite::set_total_staking_currency(
			Origin::signed(ROOT),
			lksm_issuance / 5
		));

		assert_eq!(
			HomaLite::get_staking_exchange_rate(),
			ExchangeRate::saturating_from_rational(lksm_issuance, lksm_issuance / 5)
		);
		assert_eq!(
			LiquidExchangeProvider::<Runtime>::get_exchange_rate(),
			ExchangeRate::saturating_from_rational(lksm_issuance / 5, lksm_issuance)
		);

		// The exchange rate is now 1:5 ratio
		// liquid = (1000 - 0.01) * 1_009_899_901_000_000_000 / 201_979_980_200_000_000 * 0.99
		liquid = 4_949_950_500_000_000;
		assert_ok!(HomaLite::mint(Origin::signed(BOB), amount));
		assert_eq!(Currencies::free_balance(LKSM, &BOB), liquid);

		assert_eq!(
			System::events().iter().last().unwrap().event,
			Event::HomaLite(crate::Event::Minted(BOB, amount, liquid))
		);
	});
}

#[test]
fn repeated_mints_have_similar_exchange_rate() {
	ExtBuilder::default().build().execute_with(|| {
		let amount = dollar(1000);

		assert_ok!(HomaLite::set_minting_cap(
			Origin::signed(ROOT),
			5 * dollar(INITIAL_BALANCE)
		));

		// Set the total staking amount
		let mut lksm_issuance = Currencies::total_issuance(LKSM);
		assert_eq!(lksm_issuance, dollar(1_000_000));

		// Set the exchange rate to 1(S) : 5(L)
		assert_ok!(HomaLite::set_total_staking_currency(
			Origin::signed(ROOT),
			lksm_issuance / 5
		));

		// The exchange rate is now 1:5 ratio
		// liquid = (1000 - 0.01) * 1000 / 200 * 0.99
		let liquid_1 = 4_949_950_500_000_000;
		assert_ok!(HomaLite::mint(Origin::signed(BOB), amount));
		assert_eq!(Currencies::free_balance(LKSM, &BOB), liquid_1);
		// The effective exchange rate is lower than the theoretical rate.
		assert!(liquid_1 < dollar(5000));

		// New total issuance
		lksm_issuance = Currencies::total_issuance(LKSM);
		assert_eq!(lksm_issuance, 1_004_949_950_500_000_000);
		assert_eq!(TotalStakingCurrency::<Runtime>::get(), dollar(201_000));

		// Second exchange
		// liquid = (1000 - 0.01) * 1004949.9505 / 201000 * 0.99
		let liquid_2 = 4_949_703_990_002_437;
		assert_ok!(HomaLite::mint(Origin::signed(BOB), amount));
		assert_eq!(Currencies::free_balance(LKSM, &BOB), 9_899_654_490_002_437);

		// Since the effective exchange rate is lower than the theortical rate, Liquid currency becomes more
		// valuable.
		assert!(liquid_1 > liquid_2);

		// The effective exchange rate should be quite close.
		// In this example the difffence is about 0.005%
		assert!(Permill::from_rational(liquid_1 - liquid_2, liquid_1) < Permill::from_rational(5u128, 1_000u128));

		// Now increase the Staking total by 1%
		assert_eq!(TotalStakingCurrency::<Runtime>::get(), dollar(202_000));
		assert_ok!(HomaLite::set_total_staking_currency(
			Origin::signed(ROOT),
			dollar(204_020)
		));
		lksm_issuance = Currencies::total_issuance(LKSM);
		assert_eq!(lksm_issuance, 1_009_899_654_490_002_437);

		// liquid = (1000 - 0.01) * 1009899.654490002437 / 204020 * 0.99
		let liquid_3 = 4_900_454_170_858_361;
		assert_ok!(HomaLite::mint(Origin::signed(BOB), amount));
		assert_eq!(Currencies::free_balance(LKSM, &BOB), 14_800_108_660_860_799);

		// Increasing the Staking total increases the value of Liquid currency - this makes up for the
		// staking rewards.
		assert!(liquid_3 < liquid_2);
		assert!(liquid_3 < liquid_1);
	});
}

#[test]
fn mint_fails_when_cap_is_exceeded() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(HomaLite::set_minting_cap(Origin::signed(ROOT), dollar(1_000)));

		assert_noop!(
			HomaLite::mint(Origin::signed(ALICE), dollar(1_001)),
			Error::<Runtime>::ExceededStakingCurrencyMintCap
		);

		assert_ok!(HomaLite::mint(Origin::signed(ALICE), dollar(1_000)));

		assert_noop!(
			HomaLite::mint(Origin::signed(ALICE), dollar(1)),
			Error::<Runtime>::ExceededStakingCurrencyMintCap
		);
	});
}

#[test]
fn failed_xcm_transfer_is_handled() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(HomaLite::set_minting_cap(Origin::signed(ROOT), dollar(1_000)));

		// XCM transfer fails if it is called by INVALID_CALLER.
		assert_noop!(
			HomaLite::mint(Origin::signed(INVALID_CALLER), dollar(1)),
			DispatchError::Other("invalid caller"),
		);
	});
}

#[test]
fn cannot_set_total_staking_currency_to_zero() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			HomaLite::set_total_staking_currency(Origin::signed(ROOT), 0),
			Error::<Runtime>::InvalidTotalStakingCurrency
		);
		assert_ok!(HomaLite::set_total_staking_currency(Origin::signed(ROOT), 1));
		assert_eq!(TotalStakingCurrency::<Runtime>::get(), 1);
		assert_eq!(
			System::events().iter().last().unwrap().event,
			Event::HomaLite(crate::Event::TotalStakingCurrencySet(1))
		);
	});
}

#[test]
fn can_adjust_total_staking_currency() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(HomaLite::set_total_staking_currency(Origin::signed(ROOT), 1));
		assert_eq!(HomaLite::total_staking_currency(), 1);

		assert_noop!(
			HomaLite::adjust_total_staking_currency(Origin::signed(ALICE), 5000),
			BadOrigin
		);

		// Can adjust total_staking_currency with ROOT.
		assert_ok!(HomaLite::adjust_total_staking_currency(Origin::signed(ROOT), 5000));

		assert_eq!(HomaLite::total_staking_currency(), 5001);
		assert_eq!(
			System::events().iter().last().unwrap().event,
			Event::HomaLite(crate::Event::TotalStakingCurrencySet(5001))
		);

		// Underflow / overflow causes error
		assert_noop!(
			HomaLite::adjust_total_staking_currency(Origin::signed(ROOT), -5002),
			ArithmeticError::Underflow
		);

		assert_eq!(HomaLite::total_staking_currency(), 5001);

		assert_ok!(HomaLite::set_total_staking_currency(
			Origin::signed(ROOT),
			Balance::max_value()
		));

		assert_noop!(
			HomaLite::adjust_total_staking_currency(Origin::signed(ROOT), 1),
			ArithmeticError::Overflow
		);
	});
}

#[test]
fn requires_root_to_set_total_staking_currency() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			HomaLite::set_total_staking_currency(Origin::signed(ALICE), 0),
			BadOrigin
		);
	});
}

#[test]
fn can_set_mint_cap() {
	ExtBuilder::default().build().execute_with(|| {
		// Current cap is not set
		assert_eq!(StakingCurrencyMintCap::<Runtime>::get(), 0);

		// Requires Root previlege.
		assert_noop!(
			HomaLite::set_minting_cap(Origin::signed(ALICE), dollar(1_000)),
			BadOrigin
		);

		// Set the cap.
		assert_ok!(HomaLite::set_minting_cap(Origin::signed(ROOT), dollar(1_000)));

		// Cap should be set now.
		assert_eq!(StakingCurrencyMintCap::<Runtime>::get(), dollar(1_000));

		assert_eq!(
			System::events().iter().last().unwrap().event,
			Event::HomaLite(crate::Event::StakingCurrencyMintCapUpdated(dollar(1_000)))
		);
	});
}

#[test]
fn can_set_xcm_dest_weight() {
	ExtBuilder::default().build().execute_with(|| {
		// Requires Root previlege.
		assert_noop!(
			HomaLite::set_xcm_dest_weight(Origin::signed(ALICE), 1_000_000),
			BadOrigin
		);

		// Set the cap.
		assert_ok!(HomaLite::set_xcm_dest_weight(Origin::signed(ROOT), 1_000_000));

		// Cap should be set now.
		assert_eq!(XcmDestWeight::<Runtime>::get(), 1_000_000);

		assert_eq!(
			System::events().iter().last().unwrap().event,
			Event::HomaLite(crate::Event::XcmDestWeightSet(1_000_000))
		);
	});
}

#[test]
fn mint_from_cdp_loan_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(CDPEngine::set_collateral_params(
			Origin::signed(ROOT),
			KSM,
			Change::NewValue(Some(Rate::saturating_from_rational(1, 100000))),
			Change::NewValue(Some(Ratio::saturating_from_rational(3, 2))),
			Change::NewValue(Some(Rate::saturating_from_rational(2, 10))),
			Change::NewValue(Some(Ratio::saturating_from_rational(9, 5))),
			Change::NewValue(dollar(1_000_000)),
		));
		assert_ok!(CDPEngine::set_collateral_params(
			Origin::signed(ROOT),
			LKSM,
			Change::NewValue(Some(Rate::saturating_from_rational(1, 100000))),
			Change::NewValue(Some(Ratio::saturating_from_rational(3, 2))),
			Change::NewValue(Some(Rate::saturating_from_rational(2, 10))),
			Change::NewValue(Some(Ratio::saturating_from_rational(9, 5))),
			Change::NewValue(dollar(1_000_000)),
		));
		MockPriceSource::set_relative_price(Some(Price::one()));
		assert_eq!(Currencies::free_balance(LKSM, &ALICE), 0);
		assert_eq!(Currencies::free_balance(KSM, &ALICE), dollar(1_000_000));
		assert_eq!(
			Loans::total_positions(KSM),
			Position {
				collateral: 0,
				debit: 0
			}
		);
		assert_eq!(Loans::positions(KSM, &ALICE).debit, 0);
		assert_eq!(Loans::positions(KSM, &ALICE).collateral, 0);
		assert_noop!(
			HomaLite::mint_from_cdp_loan(Origin::signed(ALICE)),
			cdp_engine::Error::<Runtime>::NoOpenPosition
		);

		assert_ok!(CDPEngine::adjust_position(
			&ALICE,
			KSM,
			dollar(100).try_into().unwrap(),
			dollar(500).try_into().unwrap()
		));
		assert_eq!(Loans::positions(KSM, &ALICE).debit, dollar(500));
		assert_eq!(Loans::positions(KSM, &ALICE).collateral, dollar(100));
		assert_eq!(Loans::total_positions(KSM).debit, dollar(500));
		assert_eq!(Loans::total_positions(KSM).collateral, dollar(100));

		assert_ok!(HomaLite::set_minting_cap(Origin::signed(ROOT), dollar(1_000_000)));
		assert_ok!(HomaLite::mint_from_cdp_loan(Origin::signed(ALICE)));

		assert_eq!(Loans::positions(KSM, &ALICE).collateral, 0);
		assert_eq!(Loans::positions(KSM, &ALICE).debit, 0);
		assert_eq!(Loans::positions(LKSM, &ALICE).debit, dollar(500));
		// About the default exchange rate of (10 Liquid / 1 Staking), or in practice mints just under dollar(1000)
		// LKSM for collateral
		let liquid_collateral_alice = Loans::positions(LKSM, &ALICE).collateral;
		assert_eq!(liquid_collateral_alice, 989901000000000);
		assert_eq!(Currencies::free_balance(LKSM, &ALICE), 0);

		// Loans has the correct total positions
		assert_eq!(Loans::total_positions(KSM).debit, 0);
		assert_eq!(Loans::total_positions(KSM).collateral, 0);
		assert_eq!(Loans::total_positions(LKSM).debit, dollar(500));
		assert_eq!(Loans::total_positions(LKSM).collateral, 989901000000000);

		// Will not work for alice again now that staking loan is closed
		assert_noop!(
			HomaLite::mint_from_cdp_loan(Origin::signed(ALICE)),
			cdp_engine::Error::<Runtime>::NoOpenPosition
		);

		// Tests that a second mint_from_cdp_loan correctly updates LKSM total_positions
		assert_eq!(Loans::positions(KSM, &BOB).debit, 0);
		assert_eq!(Loans::positions(KSM, &BOB).collateral, 0);
		assert_ok!(CDPEngine::adjust_position(
			&BOB,
			KSM,
			dollar(100).try_into().unwrap(),
			dollar(500).try_into().unwrap()
		));
		assert_eq!(Loans::positions(KSM, &BOB).debit, dollar(500));
		assert_eq!(Loans::positions(KSM, &BOB).collateral, dollar(100));
		assert_eq!(Loans::total_positions(KSM).debit, dollar(500));
		assert_eq!(Loans::total_positions(KSM).collateral, dollar(100));
		assert_eq!(Currencies::free_balance(LKSM, &BOB), 0);

		assert_ok!(HomaLite::mint_from_cdp_loan(Origin::signed(BOB)));
		assert_eq!(Loans::positions(LKSM, &BOB).debit, dollar(500));
		// The exchange rate now closer to 1000 Liquid/ 1 Staking.
		// big jump is due to Alice's using default value as staking pool was empty
		let liquid_collateral_bob = Loans::positions(LKSM, &BOB).collateral;
		assert_eq!(liquid_collateral_bob, 990880903989801000);

		assert_eq!(Loans::total_positions(KSM).debit, 0);
		assert_eq!(Loans::total_positions(KSM).collateral, 0);
		assert_eq!(Loans::total_positions(LKSM).debit, dollar(1000));
		assert_eq!(
			Loans::total_positions(LKSM).collateral,
			liquid_collateral_alice + liquid_collateral_bob
		);
		assert_eq!(Currencies::free_balance(LKSM, &BOB), 0);
	});
}

#[test]
fn mint_from_cdp_fails_correctly() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(CDPEngine::set_collateral_params(
			Origin::signed(ROOT),
			KSM,
			Change::NewValue(Some(Rate::saturating_from_rational(1, 100000))),
			Change::NewValue(Some(Ratio::saturating_from_rational(3, 2))),
			Change::NewValue(Some(Rate::saturating_from_rational(2, 10))),
			Change::NewValue(Some(Ratio::saturating_from_rational(9, 5))),
			Change::NewValue(dollar(1_000_000)),
		));
		assert_ok!(CDPEngine::set_collateral_params(
			Origin::signed(ROOT),
			LKSM,
			Change::NewValue(Some(Rate::saturating_from_rational(1, 100000))),
			Change::NewValue(Some(Ratio::saturating_from_rational(3, 2))),
			Change::NewValue(Some(Rate::saturating_from_rational(2, 10))),
			Change::NewValue(Some(Ratio::saturating_from_rational(9, 5))),
			Change::NewValue(dollar(1_000_000)),
		));
		MockPriceSource::set_relative_price(Some(Price::one()));
	});
}
