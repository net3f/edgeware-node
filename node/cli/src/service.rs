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

#![warn(unused_extern_crates)]

//! Service implementation. Specialized wrapper over substrate service.

use std::sync::Arc;

use sc_consensus_aura;
use sc_client::{self, LongestChain};
use sc_finality_grandpa::{self, FinalityProofProvider as GrandpaFinalityProofProvider, StorageAndProofProvider};
use edgeware_executor;
use edgeware_primitives::Block;
use edgeware_runtime::{RuntimeApi};
use sc_service::{
	AbstractService, ServiceBuilder, config::Configuration, error::{Error as ServiceError}
};

use sp_inherents::InherentDataProviders;
use sc_service::{Service, NetworkStatus};
use sc_client::{Client, LocalCallExecutor};
use sc_client_db::Backend;
use sp_runtime::traits::Block as BlockT;
use edgeware_executor::NativeExecutor;
use sc_network::NetworkService;
use sc_offchain::OffchainWorkers;
pub use crate::ChainSpec;

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
macro_rules! new_full_start {
	($config:expr) => {{
		use std::sync::Arc;
		type RpcExtension = jsonrpc_core::IoHandler<sc_rpc::Metadata>;
		let mut import_setup = None;
		let inherent_data_providers = sp_inherents::InherentDataProviders::new();

		let builder = sc_service::ServiceBuilder::new_full::<
			edgeware_primitives::Block, edgeware_runtime::RuntimeApi, edgeware_executor::Executor
		>($config)?
			.with_select_chain(|_config, backend| {
				Ok(sc_client::LongestChain::new(backend.clone()))
			})?
			.with_transaction_pool(|config, client, _fetcher| {
				let pool_api = sc_transaction_pool::FullChainApi::new(client.clone());
				Ok(sc_transaction_pool::BasicPool::new(config, std::sync::Arc::new(pool_api)))
			})?
			.with_import_queue(|_config, client, mut select_chain, _transaction_pool| {
				let select_chain = select_chain.take()
					.ok_or_else(|| sc_service::Error::SelectChainRequired)?;
				let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
					client.clone(),
					&(client.clone() as Arc<_>),
					select_chain,
				)?;
				let justification_import = grandpa_block_import.clone();

				let aura_block_import = sc_consensus_aura::AuraBlockImport::<_, _, _, sp_consensus_aura::ed25519::AuthorityPair>::new(
					justification_import.clone(), client.clone(),
				);

				let import_queue = sc_consensus_aura::import_queue::<_, _, _, sp_consensus_aura::ed25519::AuthorityPair>(
					sc_consensus_aura::slot_duration(&*client)?,
					aura_block_import,
					Some(Box::new(justification_import.clone())),
					None,
					client,
					inherent_data_providers.clone(),
				)?;

				import_setup = Some((grandpa_block_import, grandpa_link));
				Ok(import_queue)
			})?
			.with_rpc_extensions(|builder| -> Result<RpcExtension, _> {
				let deps = edgeware_rpc::FullDeps {
					client: builder.client().clone(),
					pool: builder.pool(),
					select_chain: builder.select_chain().cloned()
						.expect("SelectChain is present for full services or set up failed; qed."),
				};
				Ok(edgeware_rpc::create_full(deps))
			})?;

		(builder, import_setup, inherent_data_providers)
	}}
}

/// Creates a full service from the configuration.
///
/// We need to use a macro because the test suit doesn't work with an opaque service. It expects
/// concrete types instead.
macro_rules! new_full {
	($config:expr, $with_startup_data: expr) => {{
		use futures::prelude::*;
		use sc_network::Event;
		use sc_client_api::ExecutorProvider;

		let (
			is_authority,
			force_authoring,
			name,
			disable_grandpa,
			sentry_nodes,
		) = (
			$config.roles.is_authority(),
			$config.force_authoring,
			$config.name.clone(),
			$config.disable_grandpa,
			$config.network.sentry_nodes.clone(),
		);

		// sentry nodes announce themselves as authorities to the network
		// and should run the same protocols authorities do, but it should
		// never actively participate in any consensus process.
		let participates_in_consensus = is_authority && !$config.sentry_mode;

		let (builder, mut import_setup, inherent_data_providers) = new_full_start!($config);

		let service = builder
			.with_finality_proof_provider(|client, backend| {
				// GenesisAuthoritySetProvider is implemented for StorageAndProofProvider
				let provider = client as Arc<dyn sc_finality_grandpa::StorageAndProofProvider<_, _>>;
				Ok(Arc::new(sc_finality_grandpa::FinalityProofProvider::new(backend, provider)) as _)
			})?
			.build()?;

		let (block_import, grandpa_link) = import_setup.take()
				.expect("Link Half and Block Import are present for Full Services or setup failed before. qed");

		($with_startup_data)(&block_import, &grandpa_link);

		if participates_in_consensus {
			let proposer = sc_basic_authorship::ProposerFactory::new(
				service.client(),
				service.transaction_pool()
			);

			let client = service.client();
			let select_chain = service.select_chain()
				.ok_or(sc_service::Error::SelectChainRequired)?;

			let can_author_with =
				sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());

			let aura = sc_consensus_aura::start_aura::<_, _, _, _, _, sp_consensus_aura::ed25519::AuthorityPair, _, _, _>(
				sc_consensus_aura::slot_duration(&*client)?,
				client,
				select_chain,
				block_import,
				proposer,
				service.network(),
				inherent_data_providers.clone(),
				force_authoring,
				service.keystore(),
				can_author_with,
			)?;

			service.spawn_essential_task("aura-proposer", aura);

			let network = service.network();
			let dht_event_stream = network.event_stream().filter_map(|e| async move { match e {
				Event::Dht(e) => Some(e),
				_ => None,
			}}).boxed();
			let authority_discovery = sc_authority_discovery::AuthorityDiscovery::new(
				service.client(),
				network,
				sentry_nodes,
				service.keystore(),
				dht_event_stream,
				service.prometheus_registry(),
			);

			service.spawn_task("authority-discovery", authority_discovery);
		}

		// if the node isn't actively participating in consensus then it doesn't
		// need a keystore, regardless of which protocol we use below.
		let keystore = if participates_in_consensus {
			Some(service.keystore())
		} else {
			None
		};

		let config = sc_finality_grandpa::Config {
			// FIXME #1578 make this available through chainspec
			gossip_duration: std::time::Duration::from_millis(333),
			justification_period: 512,
			name: Some(name),
			observer_enabled: false,
			keystore,
			is_authority,
		};

		let enable_grandpa = !disable_grandpa;
		if enable_grandpa {
			// start the full GRANDPA voter
			// NOTE: non-authorities could run the GRANDPA observer protocol, but at
			// this point the full voter should provide better guarantees of block
			// and vote data availability than the observer. The observer has not
			// been tested extensively yet and having most nodes in a network run it
			// could lead to finality stalls.
			let grandpa_config = sc_finality_grandpa::GrandpaParams {
				config,
				link: grandpa_link,
				network: service.network(),
				inherent_data_providers: inherent_data_providers.clone(),
				telemetry_on_connect: Some(service.telemetry_on_connect_stream()),
				voting_rule: sc_finality_grandpa::VotingRulesBuilder::default().build(),
				prometheus_registry: service.prometheus_registry(),
			};

			service.spawn_essential_task(
				"grandpa-voter",
				sc_finality_grandpa::run_grandpa_voter(grandpa_config)?
			);
		} else {
			sc_finality_grandpa::setup_disabled_grandpa(
				service.client(),
				&inherent_data_providers,
				service.network(),
			)?;
		}

		Ok((service, inherent_data_providers))
	}};
	($config:expr) => {{
		new_full!($config, |_, _| {})
	}}
}


/// Concrete configuration for Edgeware runtime
type ConcreteBlock = edgeware_primitives::Block;
type ConcreteClient =
	Client<
		Backend<ConcreteBlock>,
		LocalCallExecutor<Backend<ConcreteBlock>,
		NativeExecutor<edgeware_executor::Executor>>,
		ConcreteBlock,
		edgeware_runtime::RuntimeApi
	>;
type ConcreteBackend = Backend<ConcreteBlock>;
type ConcreteTransactionPool = sc_transaction_pool::BasicPool<
	sc_transaction_pool::FullChainApi<ConcreteClient, ConcreteBlock>,
	ConcreteBlock
>;

/// Builds a new service for a full client.
pub fn new_full(config: Configuration)
-> Result<
	Service<
		ConcreteBlock,
		ConcreteClient,
		LongestChain<ConcreteBackend, ConcreteBlock>,
		NetworkStatus<ConcreteBlock>,
		NetworkService<ConcreteBlock, <ConcreteBlock as BlockT>::Hash>,
		ConcreteTransactionPool,
		OffchainWorkers<
			ConcreteClient,
			<ConcreteBackend as sc_client_api::backend::Backend<Block>>::OffchainStorage,
			ConcreteBlock,
		>
	>,
	ServiceError,
>
{
	new_full!(config).map(|(service, _)| service)
}

/// Builds a new service for a light client.
pub fn new_light(config: Configuration)
-> Result<impl AbstractService, ServiceError> {
	type RpcExtension = jsonrpc_core::IoHandler<sc_rpc::Metadata>;
	let inherent_data_providers = InherentDataProviders::new();

	let service = ServiceBuilder::new_light::<Block, RuntimeApi, edgeware_executor::Executor>(config)?
		.with_select_chain(|_config, backend| {
			Ok(LongestChain::new(backend.clone()))
		})?
		.with_transaction_pool(|config, client, fetcher| {
			let fetcher = fetcher
				.ok_or_else(|| "Trying to start light transaction pool without active fetcher")?;
			let pool_api = sc_transaction_pool::LightChainApi::new(client.clone(), fetcher.clone());
			let pool = sc_transaction_pool::BasicPool::with_revalidation_type(
				config, Arc::new(pool_api), sc_transaction_pool::RevalidationType::Light,
			);
			Ok(pool)
		})?
		.with_import_queue_and_fprb(|_config, client, backend, fetcher, _select_chain, _tx_pool| {
			let fetch_checker = fetcher
				.map(|fetcher| fetcher.checker().clone())
				.ok_or_else(|| "Trying to start light import queue without active fetch checker")?;
			let grandpa_block_import = sc_finality_grandpa::light_block_import(
				client.clone(),
				backend,
				&(client.clone() as Arc<_>),
				Arc::new(fetch_checker),
			)?;

			let finality_proof_import = grandpa_block_import.clone();
			let finality_proof_request_builder =
				finality_proof_import.create_finality_proof_request_builder();

			let import_queue = sc_consensus_aura::import_queue::<_, _, _, sp_consensus_aura::ed25519::AuthorityPair>(
				sc_consensus_aura::slot_duration(&*client)?,
				grandpa_block_import,
				None,
				Some(Box::new(finality_proof_import)),
				client,
				inherent_data_providers.clone(),
			)?;

			Ok((import_queue, finality_proof_request_builder))
		})?
		.with_finality_proof_provider(|client, backend| {
			// GenesisAuthoritySetProvider is implemented for StorageAndProofProvider
			let provider = client as Arc<dyn StorageAndProofProvider<_, _>>;
			Ok(Arc::new(GrandpaFinalityProofProvider::new(backend, provider)) as _)
		})?
		.with_rpc_extensions(|builder,| ->
			Result<RpcExtension, _>
		{
			let fetcher = builder.fetcher()
				.ok_or_else(|| "Trying to start node RPC without active fetcher")?;
			let remote_blockchain = builder.remote_backend()
				.ok_or_else(|| "Trying to start node RPC without active remote blockchain")?;

			let light_deps = edgeware_rpc::LightDeps {
				remote_blockchain,
				fetcher,
				client: builder.client().clone(),
				pool: builder.pool(),
			};
			Ok(edgeware_rpc::create_light(light_deps))
		})?
		.build()?;

	Ok(service)
}
