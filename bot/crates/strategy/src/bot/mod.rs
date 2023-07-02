use anyhow::Result;
use artemis_core::{collectors::block_collector::NewBlock, types::Strategy};
use async_trait::async_trait;
use colored::Colorize;
use ethers::{
    providers::Middleware,
    types::{Address, Transaction, U256, U64},
};
use log::{error, info};
use std::sync::Arc;

use crate::{
    log_error, log_info_cyan, log_new_block_info,
    managers::{block_manager::BlockManager, pool_manager::PoolManager},
    types::{Action, Event, StratConfig, VictimInfo},
};

pub struct SandoBot<M> {
    /// Sando inception block
    sando_inception_block: U64,
    /// Sando contract
    sando_contract: Address,
    /// Ethers client
    provider: Arc<M>,
    /// Keeps track of onchain pools
    pool_manager: PoolManager<M>,
    /// Block manager
    block_manager: BlockManager,
}

impl<M: Middleware + 'static> SandoBot<M> {
    /// Create a new instance
    pub fn new(client: Arc<M>, config: StratConfig) -> Self {
        Self {
            sando_contract: config.sando_address,
            sando_inception_block: config.sando_inception_block,
            pool_manager: PoolManager::new(client.clone()),
            provider: client,
            block_manager: BlockManager::new(),
        }
    }
}

#[async_trait]
impl<M: Middleware + 'static> Strategy<Event, Action> for SandoBot<M> {
    /// Setup by getting all pools to monitor for swaps
    async fn sync_state(&mut self) -> Result<()> {
        self.pool_manager.sync_all_pools().await?;
        self.block_manager.setup(self.provider.clone()).await?;
        Ok(())
    }

    /// Process incoming events
    async fn process_event(&mut self, event: Event) -> Option<Action> {
        match event {
            Event::NewBlock(block) => match self.process_new_block(block).await {
                Ok(_) => None,
                Err(e) => {
                    panic!("strategy is out of sync {}", e);
                }
            },
            Event::NewTransaction(tx) => self.process_new_tx(tx).await,
        }
    }
}

impl<M: Middleware + 'static> SandoBot<M> {
    /// Process new blocks as they come in
    async fn process_new_block(&mut self, event: NewBlock) -> Result<()> {
        log_new_block_info!(event);
        self.block_manager.update_block_info(event);
        Ok(())
    }

    /// Process new txs as they come in
    async fn process_new_tx(&mut self, tx: Transaction) -> Option<Action> {
        // setup variables for processing tx
        let next_block = self.block_manager.get_next_block();
        let mut victim_info = VictimInfo::new(tx, next_block);

        // ignore txs that we can't include in next block
        // enhancement: simulate all txs regardless, store result, and use result when tx can included
        if !victim_info.can_include_in_target_block() {
            log_info_cyan!("{:?} mf<nbf", victim_info.tx_args.hash);
            return None;
        }

        // get victim tx state diffs
        victim_info
            .fill_state_diffs(self.provider.clone())
            .await
            .map_err(|e| {
                log_error!("Failed to get state diffs: {}", e);
                e
            })
            .ok()?;

        // check if tx is a swap
        let touched_pools = self
            .pool_manager
            .get_touched_sandwichable_pools(&victim_info)
            .await
            .map_err(|e| {
                log_error!("Failed to get touched sandwichable pools: {}", e);
                e
            })
            .ok()?;

        // no touched pools = no sandwich opps
        if touched_pools.is_empty() {
            info!("{:?}", victim_info.tx_args.hash);
            return None;
        }

        for pool in touched_pools {
            match pool {
                cfmms::pool::Pool::UniswapV2(v2Pool) => {
                    println!("v2Pool");
                }
                cfmms::pool::Pool::UniswapV3(v3Pool) => {
                    println!("v3Pool");
                }
            }
        }

        None
    }
}
