use solana_program::program_error::ProgramError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EscrowError {
    #[error("Invalid instruction")]
    InvalidInstruction,

    #[error("Not rent exempt")]
    NotRentExempt,

    #[error("Expected amount does not match actual amount")]
    ExpectedAmountMismatch,

    #[error("Overflow when returning rent amount")]
    Overflow,
}

impl From<EscrowError> for ProgramError {
    fn from(e: EscrowError) -> Self {
        Self::Custom(e as u32)
    }
}
