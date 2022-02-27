use solana_program::program_error::ProgramError;

use crate::error::EscrowError::InvalidInstruction;

pub enum EscrowInstruction {
    /// Starts the trade by creating + populating an escrow account (transfer ownership of given temp token account to PDA)
    ///
    /// Accounts expected:
    //
    /// 0. `[signer]` Account of person who initializes escrow
    /// 1. `[writable]` Temp token account which should be created prior to instruction and owned by initializer
    /// 2. `[]` Initializer's token account for the token they receive should trade go through
    /// 3. `[writable]` Escrow account, hold all necessary info about the trade
    /// 4. `[]` Token program
    InitEscrow {
        // Amount party A expects to receive of token Y
        amount: u64,
    },

    /// Accepts a trade
    ///
    /// Accounts expected:
    //
    /// 0. `[signer]` Account of person who takes the trader
    /// 1. `[writable]` The taker's token account for the token they send
    /// 2. `[writable]` The taker's token account for the token they will receive should trade go through
    /// 3. `[writable]` PDA's temp account to get tokens from and eventually close... TODO: isn't this saved already?
    /// 4. `[writable]` Initializer's main account to send rent fees to... TODO: isn't this saved already?
    /// 5. `[writable]` Initializer's token account that will receive tokens
    /// 6. `[writable]` Escrow account holding escrow info
    /// 7. `[]` Token program
    /// 8. `[]` PDA account
    Exchange {
        // Amount taker expects to be paid in the other token, as u64 because that's the max possible supply of token
        // TODO: add expected send amount so taker can't be front-run by initializer w/ a cancel + re-initialize with higher amount.
        amount: u64,
    },
}

impl EscrowInstruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (&tag, rest) = input.split_first().ok_or(InvalidInstruction)?;

        Ok(match tag {
            0 => Self::InitEscrow {
                amount: Self::unpack_amount(rest)?,
            },
            1 => Self::Exchange {
                amount: Self::unpack_amount(rest)?,
            },
            _ => return Err(InvalidInstruction.into()),
        })
    }

    fn unpack_amount(input: &[u8]) -> Result<u64, ProgramError> {
        let amount = input
            .get(..8)
            .and_then(|slice| slice.try_into().ok())
            .map(u64::from_le_bytes)
            .ok_or(InvalidInstruction)?;
        Ok(amount)
    }
}
