use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack},
    pubkey::Pubkey,
    sysvar::rent::Rent,
};
use spl_token::state::Account as TokenAccount;

use crate::{error::EscrowError, instruction::EscrowInstruction, state::Escrow};

pub struct Processor {}

impl Processor {
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
        let instruction = EscrowInstruction::unpack(input)?;
        match instruction {
            EscrowInstruction::InitEscrow { amount } => {
                msg!("Instruction: InitEscrow");
                Self::process_init_escrow(accounts, amount, program_id)
            }
            EscrowInstruction::Exchange { amount } => {
                msg!("Instruction: Exchange");
                Self::process_exchange(accounts, amount, program_id)
            }
        }
    }

    fn process_init_escrow(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let initializer = next_account_info(account_info_iter)?;

        if !initializer.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        // No need to add check for owner since the authority transfer will check for us.
        let temp_token_account = next_account_info(account_info_iter)?;

        let dest_token_account = next_account_info(account_info_iter)?;
        if *dest_token_account.owner != spl_token::id() {
            return Err(ProgramError::IncorrectProgramId);
        }
        // Also need to check if this is a token account by unpacking it
        TokenAccount::unpack(&dest_token_account.try_borrow_data()?)?;

        // We initialize our escrow account data here.

        let escrow_account = next_account_info(account_info_iter)?;
        // Old way of doing things (w/ sysvar rent account as input).
        // let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
        // if !rent.is_exempt(escrow_account.lamports(), escrow_account.data_len()) {
        //     return Err(EscrowError::NotRentExempt.into());
        // }

        // New way of doing things.
        if !Rent::is_exempt(
            &Rent::default(),
            escrow_account.lamports(),
            escrow_account.data_len(),
        ) {
            return Err(EscrowError::NotRentExempt.into());
        }

        let mut escrow_info = Escrow::unpack_unchecked(&escrow_account.try_borrow_data()?)?;
        if escrow_info.is_initialized() {
            return Err(ProgramError::AccountAlreadyInitialized);
        }

        escrow_info.is_initialized = true;
        escrow_info.initializer_pubkey = *initializer.key;
        escrow_info.temp_token_account_pubkey = *temp_token_account.key;
        escrow_info.initializer_dest_token_account_pubkey = *dest_token_account.key;
        escrow_info.expected_amount = amount;

        Escrow::pack(escrow_info, &mut escrow_account.try_borrow_mut_data()?)?;

        // Transfer ownership of temp token account to Escrow program.

        let (pda, _bump_seed) = Pubkey::find_program_address(&[b"escrow"], program_id);
        let token_program = next_account_info(account_info_iter)?;
        let owner_change_ix = spl_token::instruction::set_authority(
            token_program.key,
            temp_token_account.key,
            Some(&pda),
            spl_token::instruction::AuthorityType::AccountOwner,
            initializer.key,
            &[initializer.key],
        )?;

        msg!("Calling token program to transfer token account ownership...");
        invoke(
            &owner_change_ix,
            &[
                temp_token_account.clone(),
                initializer.clone(),
                token_program.clone(),
            ],
        )?;

        Ok(())
    }

    fn process_exchange(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();

        let taker = next_account_info(account_info_iter)?;
        let taker_source_token_account = next_account_info(account_info_iter)?;
        let taker_dest_token_account = next_account_info(account_info_iter)?;
        let temp_token_account = next_account_info(account_info_iter)?;
        let initializer = next_account_info(account_info_iter)?;
        let initializer_dest_token_account = next_account_info(account_info_iter)?;
        let escrow_account = next_account_info(account_info_iter)?;
        let token_program = next_account_info(account_info_iter)?;
        let pda_account = next_account_info(account_info_iter)?;
        // No need to check for ownership since we'll write to it later.
        let escrow = Escrow::unpack(&escrow_account.try_borrow_data()?)?;

        // I think we check this because we never explicitly transfer out of taker, so we need to
        // check that taker is authorized(?)
        if !taker.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        // Check everything matches up with our escrow.

        if *temp_token_account.key != escrow.temp_token_account_pubkey {
            return Err(ProgramError::InvalidAccountData);
        }
        if *initializer.key != escrow.initializer_pubkey {
            return Err(ProgramError::InvalidAccountData);
        }
        if *initializer_dest_token_account.key != escrow.initializer_dest_token_account_pubkey {
            return Err(ProgramError::InvalidAccountData);
        }

        let temp_token_account_info = TokenAccount::unpack(&temp_token_account.try_borrow_data()?)?;
        if temp_token_account_info.amount != amount {
            return Err(EscrowError::ExpectedAmountMismatch.into());
        }

        // Transfer tokens from taker to initializer.

        let transfer_to_initializer = spl_token::instruction::transfer(
            token_program.key,
            taker_source_token_account.key,
            initializer_dest_token_account.key,
            taker.key,
            &[taker.key],
            escrow.expected_amount,
        )?;
        msg!("Calling token program to transfer tokens to escrow's initializer...");
        invoke(
            &transfer_to_initializer,
            &[
                taker_source_token_account.clone(),
                initializer_dest_token_account.clone(),
                taker.clone(),
                // NB: this is not necessary it seems.
                // token_program.clone(),
            ],
        )?;

        let (pda, bump_seed) = Pubkey::find_program_address(&[b"escrow"], program_id);

        // Transfer tokens from initializer's temp account to taker.

        let transfer_to_taker_ix = spl_token::instruction::transfer(
            token_program.key,
            temp_token_account.key,
            taker_dest_token_account.key,
            // Do we need to generate a
            &pda,
            &[&pda],
            // pda_account.key,
            // &[pda_account],
            amount,
        )?;
        msg!("Calling token program to transfer tokens to the taker...");
        invoke_signed(
            &transfer_to_taker_ix,
            &[
                temp_token_account.clone(),
                taker_dest_token_account.clone(),
                // I think this will implicitly check that pda == pda_account(?)
                pda_account.clone(),
                // NB: this is not necessary it seems.
                // token_program.clone(),
            ],
            &[&[&b"escrow"[..], &[bump_seed]]],
        )?;

        // Close temp token account created when escrow was initialized.

        let close_account_ix = spl_token::instruction::close_account(
            token_program.key,
            temp_token_account.key,
            initializer.key,
            &pda,
            &[&pda],
        )?;
        msg!("Calling token program to close pda's temp account...");
        invoke_signed(
            &close_account_ix,
            &[
                temp_token_account.clone(),
                initializer.clone(),
                pda_account.clone(),
                // NB: this is not necessary it seems.
                // token_program.clone(),
            ],
            &[&[&b"escrow"[..], &[bump_seed]]],
        )?;

        msg!("Closing the escrow account...");
        **initializer.lamports.borrow_mut() = initializer
            .lamports()
            .checked_add(escrow_account.lamports())
            .ok_or(EscrowError::Overflow)?;
        **escrow_account.lamports.borrow_mut() = 0;
        *escrow_account.try_borrow_mut_data()? = &mut [];

        Ok(())
    }
}
