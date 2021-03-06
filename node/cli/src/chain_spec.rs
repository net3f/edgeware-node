// Copyright 2018-2020 Commonwealth Labs, Inc.
// This file is part of Edgeware.

// Edgeware is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Edgeware is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Edgeware.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate chain configurations.

use sc_chain_spec::ChainSpecExtension;
use sp_core::{Pair, Public, sr25519};
use serde::{Serialize, Deserialize};
use edgeware_runtime::{
	AuthorityDiscoveryConfig, AuraConfig, BalancesConfig, ContractsConfig, CouncilConfig, DemocracyConfig,
	GrandpaConfig, ImOnlineConfig, IndicesConfig, SessionConfig, SessionKeys, StakerStatus, StakingConfig,
	SudoConfig, SystemConfig, VestingConfig, WASM_BINARY,
	SignalingConfig, TreasuryRewardConfig,
	// EVMConfig
};
use edgeware_runtime::Block;
use edgeware_runtime::constants::currency::*;
use sc_service;

use sc_telemetry::TelemetryEndpoints;
use sp_finality_grandpa::{AuthorityId as GrandpaId};
use sp_consensus_aura::ed25519::AuthorityId as AuraId;
use pallet_im_online::ed25519::{AuthorityId as ImOnlineId};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_runtime::{Perbill, traits::{Verify, IdentifyAccount, One}};

pub use edgeware_primitives::{AccountId, Balance, Signature, BlockNumber};
pub use edgeware_runtime::GenesisConfig;
pub use edgeware_runtime::constants::{time::*};

use std::fs::File;
use std::io::Read;
use std::path::Path;
use serde_json::{Result};
use hex::FromHex;

type AccountPublic = <Signature as Verify>::Signer;

const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";
const DEFAULT_PROTOCOL_ID: &str = "edg";

#[derive(Serialize, Deserialize, Debug)]
struct Allocation {
    balances: Vec<(String, String)>,
    vesting: Vec<(String, BlockNumber, BlockNumber, String)>,
}

fn get_lockdrop_participants_allocation() -> Result<Allocation>{
	let path = Path::new("node/cli/lockdrop/lockdrop-allocation.json");
	let mut file = File::open(&path).unwrap();
	let mut data = String::new();
	file.read_to_string(&mut data).unwrap();
	let a: Allocation = serde_json::from_str(&data)?;
	return Ok(a);
}

/// Node `ChainSpec` extensions.
///
/// Additional parameters for some Substrate core modules,
/// customizable from the chain spec.
#[derive(Default, Clone, Serialize, Deserialize, ChainSpecExtension)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
	/// Block numbers with known hashes.
	pub fork_blocks: sc_client::ForkBlocks<Block>,
	/// Known bad block hashes.
	pub bad_blocks: sc_client::BadBlocks<Block>,
}

/// Specialized `ChainSpec`.
pub type ChainSpec = sc_service::GenericChainSpec<
	GenesisConfig,
	Extensions,
>;

/// Mainnet configuration
pub fn edgeware_mainnet_official() -> ChainSpec {
	match ChainSpec::from_json_file(std::path::PathBuf::from("chains/mainnet.chainspec.json")) {
		Ok(spec) => spec,
		Err(e) => panic!(e),
	}
}

/// 0.9.9 Testnet configuration
pub fn edgeware_testnet_v099_config() -> ChainSpec {
	match ChainSpec::from_json_file(std::path::PathBuf::from("chains/testnet-0.9.9.chainspec.json")) {
		Ok(spec) => spec,
		Err(e) => panic!(e),
	}
}

/// 1.0.0 Berlin Testnet configuration
pub fn edgeware_berlin_testnet_config() -> ChainSpec {
	match ChainSpec::from_json_file(std::path::PathBuf::from("chains/berlin.chainspec.json")) {
		Ok(spec) => spec,
		Err(e) => panic!(e),
	}
}

fn session_keys(
	grandpa: GrandpaId,
	aura: AuraId,
	im_online: ImOnlineId,
	authority_discovery: AuthorityDiscoveryId,
) -> SessionKeys {
	SessionKeys { grandpa, aura, im_online, authority_discovery }
}

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Helper function to generate stash, controller and session key from seed
pub fn get_authority_keys_from_seed(seed: &str) -> (
	AccountId,
	AccountId,
	GrandpaId,
	AuraId,
	ImOnlineId,
	AuthorityDiscoveryId,
) {
	(
		get_account_id_from_seed::<sr25519::Public>(&format!("{}//stash", seed)),
		get_account_id_from_seed::<sr25519::Public>(seed),
		get_from_seed::<GrandpaId>(seed),
		get_from_seed::<AuraId>(seed),
		get_from_seed::<ImOnlineId>(seed),
		get_from_seed::<AuthorityDiscoveryId>(seed),
	)
}

/// Helper function to create GenesisConfig for testing
pub fn testnet_genesis(
	initial_authorities: Vec<(AccountId, AccountId, GrandpaId, AuraId, ImOnlineId, AuthorityDiscoveryId)>,
	_root_key: AccountId,
	endowed_accounts: Option<Vec<AccountId>>,
	enable_println: bool,
	balances: Vec<(AccountId, Balance)>,
	vesting: Vec<(AccountId, BlockNumber, BlockNumber, Balance)>,
	founder_allocation: Vec<(AccountId, Balance)>,
) -> GenesisConfig {
	let endowed_accounts: Vec<AccountId> = endowed_accounts.unwrap_or_else(|| {
		vec![
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			get_account_id_from_seed::<sr25519::Public>("Bob"),
			get_account_id_from_seed::<sr25519::Public>("Charlie"),
			get_account_id_from_seed::<sr25519::Public>("Dave"),
			get_account_id_from_seed::<sr25519::Public>("Eve"),
			get_account_id_from_seed::<sr25519::Public>("Ferdie"),
			get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
			get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
			get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
			get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
			get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
			get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
		]
	});

	const STASH: Balance = 100 * DOLLARS;

	GenesisConfig {
		frame_system: Some(SystemConfig {
			code: WASM_BINARY.to_vec(),
			changes_trie_config: Default::default(),
		}),
		pallet_balances: Some(BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, STASH))
				.chain(founder_allocation.iter().map(|x| (x.0.clone(), x.1.clone())))
				.chain(initial_authorities.iter().map(|x| (x.0.clone(), STASH)))
				.chain(initial_authorities.iter().map(|x| (x.1.clone(), STASH)))
				.chain(balances.clone())
				.collect(),
		}),
		pallet_indices: Some(IndicesConfig {
			indices: vec![],
		}),
		pallet_session: Some(SessionConfig {
			keys: initial_authorities.iter().map(|x| {
				(x.0.clone(), x.0.clone(), session_keys(x.2.clone(), x.3.clone(), x.4.clone(), x.5.clone()))
			}).collect::<Vec<_>>(),
		}),
		pallet_staking: Some(StakingConfig {
			validator_count: 20,
			minimum_validator_count: initial_authorities.len() as u32,
			stakers: initial_authorities.iter().map(|x| {
				(x.0.clone(), x.1.clone(), STASH, StakerStatus::Validator)
			}).collect(),
			invulnerables: [].to_vec(),
			slash_reward_fraction: Perbill::from_percent(10),
			.. Default::default()
		}),
		pallet_democracy: Some(DemocracyConfig::default()),
		pallet_collective_Instance1: Some(CouncilConfig {
			members: crate::testnet_fixtures::get_testnet_election_members(),
			phantom: Default::default(),
		}),
		pallet_contracts: Some(ContractsConfig {
			current_schedule: pallet_contracts::Schedule {
				enable_println, // this should only be enabled on development chains
				..Default::default()
			},
			gas_price: 1 * MILLICENTS,
		}),
		pallet_aura: Some(AuraConfig {
			authorities: vec![],
		}),
		pallet_im_online: Some(ImOnlineConfig {
			keys: vec![],
		}),
		pallet_authority_discovery: Some(AuthorityDiscoveryConfig {
			keys: vec![],
		}),
		pallet_grandpa: Some(GrandpaConfig {
			authorities: vec![],
		}),
		pallet_treasury: Some(Default::default()),
		signaling: Some(SignalingConfig {
			voting_length: 7 * DAYS,
			proposal_creation_bond: 1 * DOLLARS,
		}),
		treasury_reward: Some(TreasuryRewardConfig {
			current_payout: 95 * DOLLARS,
			minting_interval: One::one(),
		}),
		pallet_sudo: Some(SudoConfig {
			key: _root_key,
		}),
		pallet_vesting: Some(VestingConfig {
			vesting: vesting,
		}),
		// pallet_evm: Some(EVMConfig {
		// 	accounts: vec![],
		// }),
	}
}

fn edgeware_testnet_config_genesis() -> GenesisConfig {
	let allocation = get_lockdrop_participants_allocation().unwrap();
	let balances = allocation.balances.iter().map(|b| {
		let balance = b.1.to_string().parse::<Balance>().unwrap();
		return (
			<[u8; 32]>::from_hex(b.0.clone()).unwrap().into(),
			balance,
		);
	})
	.filter(|b| b.1 > 0)
	.collect();
	let vesting = allocation.vesting.iter().map(|b| {
		let vesting_balance = b.3.to_string().parse::<Balance>().unwrap();
		return (
			(<[u8; 32]>::from_hex(b.0.clone()).unwrap()).into(),
			b.1,
			b.2,
			vesting_balance,
		);
	})
	.filter(|b| b.3 > 0)
	.collect();

	let initial_authorities = crate::testnet_fixtures::get_mtestnet_initial_authorities();

	testnet_genesis(
		initial_authorities,
		crate::testnet_fixtures::get_testnet_root_key(),
		None,
		true,
		balances,
		vesting,
		crate::mainnet_fixtures::get_commonwealth_allocation(),
	)
}

/// Edgeware config (8 validators)
pub fn edgeware_testnet_config(testnet_name: String, testnet_node_name: String) -> ChainSpec {
	let data = r#"
		{
			"ss58Format": 42,
			"tokenDecimals": 18,
			"tokenSymbol": "tEDG"
		}"#;
	let properties = serde_json::from_str(data).unwrap();
	let boot_nodes = crate::testnet_fixtures::get_mtestnet_bootnodes();
	ChainSpec::from_genesis(
		&testnet_name,
		&testnet_node_name,
		edgeware_testnet_config_genesis,
		boot_nodes,
		Some(TelemetryEndpoints::new(vec![(STAGING_TELEMETRY_URL.to_string(), 0)])),
		Some(DEFAULT_PROTOCOL_ID),
		properties,
		Default::default(),
	)
}

fn multi_development_config_genesis() -> GenesisConfig {
	testnet_genesis(
		vec![
			get_authority_keys_from_seed("Alice"),
			get_authority_keys_from_seed("Bob"),
			get_authority_keys_from_seed("Charlie"),
			get_authority_keys_from_seed("Dave"),
			get_authority_keys_from_seed("Eve"),
			get_authority_keys_from_seed("Ferdie"),
		],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
		true,
		vec![],
		vec![],
		vec![],
	)
}

fn development_config_genesis() -> GenesisConfig {
	testnet_genesis(
		vec![
			get_authority_keys_from_seed("Alice"),
		],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
		true,
		vec![],
		vec![],
		vec![],
	)
}

/// Development config (single validator Alice)
pub fn development_config() -> ChainSpec {
	let data = r#"
		{
			"ss58Format": 42,
			"tokenDecimals": 18,
			"tokenSymbol": "tEDG"
		}"#;
	let properties = serde_json::from_str(data).unwrap();
	ChainSpec::from_genesis(
		"Development",
		"dev",
		development_config_genesis,
		vec![],
		None,
		None,
		properties,
		Default::default(),
	)
}

/// Development config (6 validators Alice, Bob, Charlie, Dave, Eve, Ferdie)
pub fn multi_development_config() -> ChainSpec {
	let data = r#"
		{
			"ss58Format": 42,
			"tokenDecimals": 18,
			"tokenSymbol": "tEDG"
		}"#;
	let properties = serde_json::from_str(data).unwrap();
	ChainSpec::from_genesis(
		"Multi Development",
		"multi-dev",
		multi_development_config_genesis,
		vec![],
		None,
		None,
		properties,
		Default::default(),
	)
}

fn local_testnet_genesis() -> GenesisConfig {
	testnet_genesis(
		vec![
			get_authority_keys_from_seed("Alice"),
			get_authority_keys_from_seed("Bob"),
		],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
		false,
		vec![],
		vec![],
		vec![],
	)
}

/// Local testnet config (multivalidator Alice + Bob)
pub fn local_testnet_config() -> ChainSpec {
	ChainSpec::from_genesis(
		"Local Testnet",
		"local_testnet",
		local_testnet_genesis,
		vec![],
		None,
		None,
		None,
		Default::default(),
	)
}

/// Helper function to create GenesisConfig for testing
pub fn mainnet_genesis(
	initial_authorities: Vec<(AccountId, AccountId, Balance, AuraId, GrandpaId, ImOnlineId, AuthorityDiscoveryId)>,
	founder_allocation: Vec<(AccountId, Balance)>,
	balances: Vec<(AccountId, Balance)>,
	vesting: Vec<(AccountId, BlockNumber, BlockNumber, Balance)>,
) -> GenesisConfig {
	let enable_println = false;
	GenesisConfig {
		frame_system: Some(SystemConfig {
			code: WASM_BINARY.to_vec(),
			changes_trie_config: Default::default(),
		}),
		pallet_balances: Some(BalancesConfig {
			balances: founder_allocation.iter().map(|x| (x.0.clone(), x.1.clone()))
				.chain(balances.clone())
				.collect(),
		}),
		pallet_indices: Some(IndicesConfig {
			indices: vec![],
		}),
		pallet_session: Some(SessionConfig {
			keys: initial_authorities.iter().map(|x| {
				(x.0.clone(), x.0.clone(), session_keys(x.4.clone(), x.3.clone(), x.5.clone(), x.6.clone()))
			}).collect::<Vec<_>>(),
		}),
		pallet_staking: Some(StakingConfig {
			validator_count: 60,
			minimum_validator_count: initial_authorities.len() as u32,
			stakers: initial_authorities.iter().map(|x| {
				(x.0.clone(), x.1.clone(), x.2.clone(), StakerStatus::Validator)
			}).collect(),
			invulnerables: vec![],
			slash_reward_fraction: Perbill::from_percent(10),
			.. Default::default()
		}),
		pallet_democracy: Some(DemocracyConfig::default()),
		pallet_collective_Instance1: Some(CouncilConfig {
			members: crate::mainnet_fixtures::get_mainnet_election_members(),
			phantom: Default::default(),
		}),
		pallet_contracts: Some(ContractsConfig {
			current_schedule: pallet_contracts::Schedule {
				enable_println, // this should only be enabled on development chains
				..Default::default()
			},
			gas_price: 1 * MILLICENTS,
		}),
		pallet_aura: Some(AuraConfig {
			authorities: vec![],
		}),
		pallet_im_online: Some(ImOnlineConfig {
			keys: vec![],
		}),
		pallet_authority_discovery: Some(AuthorityDiscoveryConfig {
			keys: vec![],
		}),
		pallet_grandpa: Some(GrandpaConfig {
			authorities: vec![],
		}),
		pallet_treasury: Some(Default::default()),
		signaling: Some(SignalingConfig {
			voting_length: 7 * DAYS,
			proposal_creation_bond: 100 * DOLLARS,
		}),
		treasury_reward: Some(TreasuryRewardConfig {
			current_payout: 95 * DOLLARS,
			minting_interval: One::one(),
		}),
		pallet_sudo: Some(SudoConfig {
			key: crate::mainnet_fixtures::get_mainnet_root_key(),
		}),
		pallet_vesting: Some(VestingConfig {
			vesting: vesting,
		}),
		// pallet_evm: Some(EVMConfig {
		// 	accounts: vec![],
		// }),
	}
}


/// Mainnet config
fn edgeware_mainnet_config_genesis() -> GenesisConfig {
	let allocation = get_lockdrop_participants_allocation().unwrap();
	let balances = allocation.balances.iter().map(|b| {
		let balance = b.1.to_string().parse::<Balance>().unwrap();
		return (
			<[u8; 32]>::from_hex(b.0.clone()).unwrap().into(),
			balance,
		);
	})
	.filter(|b| b.1 > 0)
	.collect();
	let vesting = allocation.vesting.iter().map(|b| {
		let vesting_balance = b.3.to_string().parse::<Balance>().unwrap();
		return (
			(<[u8; 32]>::from_hex(b.0.clone()).unwrap()).into(),
			b.1,
			b.2,
			vesting_balance,
		);
	})
	.filter(|b| b.3 > 0)
	.collect();

	mainnet_genesis(
		crate::mainnet_fixtures::get_cw_mainnet_validators(),
		crate::mainnet_fixtures::get_commonwealth_allocation(),
		balances,
		vesting,
	)
}

/// Edgeware config (8 validators)
pub fn edgeware_mainnet_config() -> ChainSpec {
	let data = r#"
		{
			"ss58Format": 7,
			"tokenDecimals": 18,
			"tokenSymbol": "EDG"
		}"#;
	let properties = serde_json::from_str(data).unwrap();
	let boot_nodes = crate::mainnet_fixtures::get_mainnet_bootnodes();
	ChainSpec::from_genesis(
		"Edgeware",
		"edgeware",
		edgeware_mainnet_config_genesis,
		boot_nodes,
		Some(TelemetryEndpoints::new(vec![(STAGING_TELEMETRY_URL.to_string(), 0)])),
		Some(DEFAULT_PROTOCOL_ID),
		properties,
		Default::default(),
	)
}
