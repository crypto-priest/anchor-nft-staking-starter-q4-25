use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{mint_to, Mint, MintTo, Token, TokenAccount},
};

use crate::state::{StakeConfig, UserAccount};

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = reward_mint,
        associated_token::authority = user,
    )]
    pub rewards_ata: Account<'info, TokenAccount>,

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

    #[account(
        mut,
        seeds = [b"rewards", config.key().as_ref()],
        bump = config.rewards_bump,
    )]
    pub reward_mint: Account<'info, Mint>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

impl<'info> Claim<'info> {
    pub fn claim(&mut self) -> Result<()> {
        // Calculate tokens to mint: points * 10^decimals (1 point = 1 token)
        let decimals = self.reward_mint.decimals;
        let tokens_to_mint = (self.user_account.points as u64)
            .checked_mul(10u64.pow(decimals as u32))
            .unwrap_or(0);

        if tokens_to_mint > 0 {
            // Create signer seeds for the config PDA (mint authority)
            let signer_seeds: &[&[&[u8]]] = &[&[
                b"config",
                &[self.config.bump],
            ]];

            // Mint reward tokens to user
            mint_to(
                CpiContext::new_with_signer(
                    self.token_program.to_account_info(),
                    MintTo {
                        mint: self.reward_mint.to_account_info(),
                        to: self.rewards_ata.to_account_info(),
                        authority: self.config.to_account_info(),
                    },
                    signer_seeds,
                ),
                tokens_to_mint,
            )?;
        }

        // Reset user's points to 0
        self.user_account.points = 0;

        Ok(())
    }
}
