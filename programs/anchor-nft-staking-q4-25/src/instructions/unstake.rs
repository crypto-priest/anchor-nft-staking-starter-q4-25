use anchor_lang::prelude::*;
use mpl_core::{
    accounts::BaseAssetV1,
    instructions::UpdatePluginV1CpiBuilder,
    types::{FreezeDelegate, Plugin},
    ID as CORE_PROGRAM_ID,
};

use crate::{
    errors::StakeError,
    state::{CollectionInfo, StakeAccount, StakeConfig, UserAccount},
};

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        constraint = asset.owner == &CORE_PROGRAM_ID @ StakeError::InvalidAsset,
        constraint = !asset.data_is_empty() @ StakeError::AssetNotInitialized,
    )]
    /// CHECK: Verified by mpl-core
    pub asset: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = collection.owner == &CORE_PROGRAM_ID @ StakeError::InvalidCollection,
        constraint = !collection.data_is_empty() @ StakeError::CollectionNotInitialized,
    )]
    /// CHECK: Verified by mpl-core
    pub collection: UncheckedAccount<'info>,

    #[account(
        seeds = [b"collection_info", collection.key().as_ref()],
        bump = collection_info.bump,
    )]
    pub collection_info: Account<'info, CollectionInfo>,

    #[account(
        mut,
        close = user,
        seeds = [b"stake", config.key().as_ref(), asset.key().as_ref()],
        bump = stake_account.bump,
        constraint = stake_account.owner == user.key() @ StakeError::NotOwner,
        constraint = stake_account.mint == asset.key() @ StakeError::InvalidAsset,
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, StakeConfig>,

    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(address = CORE_PROGRAM_ID)]
    /// CHECK: Verified by address constraint
    pub core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> Unstake<'info> {
    pub fn unstake(&mut self) -> Result<()> {
        let clock = Clock::get()?;
        let time_elapsed = clock.unix_timestamp - self.stake_account.staked_at;

        // Check freeze period has passed
        require!(
            time_elapsed >= self.config.freeze_period as i64,
            StakeError::FreezePeriodNotPassed
        );

        // Verify the user owns the asset
        let asset_data = BaseAssetV1::try_from(&self.asset.to_account_info())?;
        require!(
            asset_data.owner == self.user.key(),
            StakeError::NotOwner
        );

        // Calculate points earned: (time_elapsed / freeze_period) * points_per_stake
        // If freeze_period is 0, award points_per_stake for any staking duration
        let points_earned = if self.config.freeze_period == 0 {
            self.config.points_per_stake as u32
        } else {
            let periods = time_elapsed as u32 / self.config.freeze_period;
            periods.saturating_mul(self.config.points_per_stake as u32)
        };

        // Update user's points
        self.user_account.points = self.user_account.points.saturating_add(points_earned);

        // Decrement user's staked amount
        self.user_account.amount_staked = self.user_account.amount_staked.saturating_sub(1);

        // Signer seeds for collection_info PDA (the update authority)
        let collection_key = self.collection.key();
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"collection_info",
            collection_key.as_ref(),
            &[self.collection_info.bump],
        ]];

        // Unfreeze the asset by updating the FreezeDelegate plugin
        // Note: The plugin remains but is set to unfrozen, allowing transfers
        UpdatePluginV1CpiBuilder::new(&self.core_program.to_account_info())
            .asset(&self.asset.to_account_info())
            .collection(Some(&self.collection.to_account_info()))
            .payer(&self.user.to_account_info())
            .authority(Some(&self.collection_info.to_account_info()))
            .system_program(&self.system_program.to_account_info())
            .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
            .invoke_signed(signer_seeds)?;

        // stake_account is closed automatically by the `close = user` constraint

        Ok(())
    }
}
