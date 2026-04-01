use litesvm::LiteSVM;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction, InstructionError},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::{Transaction, TransactionError},
};

// ── Constants (must match offsets.py) ────────────────────────────────

const MINT_SZ: usize = 40;
const TOKEN_SZ: usize = 72;
const MA_TOTAL_MINTED: usize = 32;
const TA_BALANCE: usize = 0x40;
const IX_MINT_TO: u8 = 0;

// ── Error codes (must match token_program.s) ──────────────────────────

const ERR_INVALID_IX: u32 = 0x01;
const ERR_WRONG_ACCT_COUNT: u32 = 0x02;
const ERR_NOT_SIGNER: u32 = 0x03;
const ERR_NOT_WRITABLE: u32 = 0x04;
const ERR_WRONG_ACCT_SIZE: u32 = 0x05;
const ERR_MINT_MISMATCH: u32 = 0x06;
const ERR_OVERFLOW: u32 = 0x09;
const ERR_ZERO_AMOUNT: u32 = 0x0a;

// ── Program ID (bytes 32..64 of deploy/token_program-keypair.json) ───

const PROGRAM_ID_BYTES: [u8; 32] = [
    100, 90, 150, 68, 223, 75, 128, 162, 220, 199, 20, 12, 62, 211, 220, 12,
    44, 32, 101, 98, 225, 98, 2, 157, 66, 186, 18, 61, 84, 57, 104, 122,
];

// ── Helpers ───────────────────────────────────────────────────────────

fn setup() -> (LiteSVM, Pubkey) {
    let mut svm = LiteSVM::new();
    let program_id = Pubkey::from(PROGRAM_ID_BYTES);
    svm.add_program_from_file(program_id, "deploy/token_program.so")
        .expect("failed to load token_program.so — build it first");
    (svm, program_id)
}

fn mint_account(program_id: Pubkey, mint_key: &Pubkey) -> Account {
    // MA_AUTHORITY (32b) = mint_key, MA_TOTAL_MINTED (8b) = 0
    let mut data = vec![0u8; MINT_SZ];
    data[0..32].copy_from_slice(&mint_key.to_bytes());
    Account {
        lamports: 1_141_440,
        data,
        owner: program_id,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

fn token_data(mint: &Pubkey, authority: &Pubkey, balance: u64) -> Vec<u8> {
    let mut d = vec![0u8; TOKEN_SZ];
    d[0..32].copy_from_slice(&mint.to_bytes());
    d[32..64].copy_from_slice(&authority.to_bytes());
    d[64..72].copy_from_slice(&balance.to_le_bytes());
    d
}

fn token_account(program_id: Pubkey, data: Vec<u8>) -> Account {
    Account {
        lamports: 1_141_440,
        data,
        owner: program_id,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

fn mint_to_ix(
    program_id: Pubkey,
    mint: Pubkey,
    token: Pubkey,
    amount: u64,
) -> Instruction {
    let mut data = vec![IX_MINT_TO];
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction::new_with_bytes(
        program_id,
        &data,
        vec![
            AccountMeta::new(mint, true),
            AccountMeta::new(token, false),
        ],
    )
}

fn custom_err(code: u32) -> TransactionError {
    TransactionError::InstructionError(0, InstructionError::Custom(code))
}

fn print_logs(
    label: &str,
    result: &Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata>,
) {
    let logs = match result {
        Ok(meta) => &meta.logs,
        Err(e) => &e.meta.logs,
    };
    println!("[{}]", label);
    for log in logs {
        println!("  {}", log);
    }
}

fn send(
    svm: &mut LiteSVM,
    ix: Instruction,
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        signers,
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[test]
fn test_mint_to_happy_path() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 0))).unwrap();

    let ix = mint_to_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), 1_000_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &mint_kp]);
    print_logs("mint_to_happy_path", &result);
    result.unwrap();

    let mint = svm.get_account(&mint_kp.pubkey()).unwrap();
    let token = svm.get_account(&token_kp.pubkey()).unwrap();
    let total_minted = u64::from_le_bytes(mint.data[MA_TOTAL_MINTED..MA_TOTAL_MINTED + 8].try_into().unwrap());
    let balance = u64::from_le_bytes(token.data[TA_BALANCE..TA_BALANCE + 8].try_into().unwrap());
    assert_eq!(total_minted, 1_000_000);
    assert_eq!(balance, 1_000_000);
}

#[test]
fn test_mint_to_accumulates() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 500_000))).unwrap();

    let ix = mint_to_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), 1_000_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &mint_kp]);
    print_logs("mint_to_accumulates", &result);
    result.unwrap();

    let token = svm.get_account(&token_kp.pubkey()).unwrap();
    let balance = u64::from_le_bytes(token.data[TA_BALANCE..TA_BALANCE + 8].try_into().unwrap());
    assert_eq!(balance, 1_500_000);
}

#[test]
fn test_mint_to_wrong_mint() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let wrong_mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();
    svm.set_account(wrong_mint_kp.pubkey(), mint_account(program_id, &wrong_mint_kp.pubkey())).unwrap();
    // token points to mint_kp, but we pass wrong_mint_kp
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 0))).unwrap();

    let ix = mint_to_ix(program_id, wrong_mint_kp.pubkey(), token_kp.pubkey(), 1_000_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &wrong_mint_kp]);
    print_logs("mint_to_wrong_mint", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_MINT_MISMATCH));
}

#[test]
fn test_mint_to_not_signer() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 0))).unwrap();

    // mint not a signer
    let mut data = vec![IX_MINT_TO];
    data.extend_from_slice(&1_000_000u64.to_le_bytes());
    let ix = Instruction::new_with_bytes(
        program_id,
        &data,
        vec![
            AccountMeta::new(mint_kp.pubkey(), false), // not signer
            AccountMeta::new(token_kp.pubkey(), false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("mint_to_not_signer", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_SIGNER));
}

#[test]
fn test_mint_to_not_writable() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 0))).unwrap();

    // token not writable
    let mut data = vec![IX_MINT_TO];
    data.extend_from_slice(&1_000_000u64.to_le_bytes());
    let ix = Instruction::new_with_bytes(
        program_id,
        &data,
        vec![
            AccountMeta::new(mint_kp.pubkey(), true),
            AccountMeta::new_readonly(token_kp.pubkey(), false), // not writable
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer, &mint_kp]);
    print_logs("mint_to_not_writable", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_WRITABLE));
}

#[test]
fn test_mint_to_wrong_acct_count() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();

    // only 1 account — missing token
    let mut data = vec![IX_MINT_TO];
    data.extend_from_slice(&1_000_000u64.to_le_bytes());
    let ix = Instruction::new_with_bytes(
        program_id,
        &data,
        vec![AccountMeta::new(mint_kp.pubkey(), true)],
    );
    let result = send(&mut svm, ix, &payer, &[&payer, &mint_kp]);
    print_logs("mint_to_wrong_acct_count", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_COUNT));
}

#[test]
fn test_mint_to_wrong_mint_size() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    // wrong mint size: 8 instead of 40
    svm.set_account(mint_kp.pubkey(), token_account(program_id, vec![0u8; 8])).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 0))).unwrap();

    let ix = mint_to_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), 1_000_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &mint_kp]);
    print_logs("mint_to_wrong_mint_size", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_SIZE));
}

#[test]
fn test_mint_to_wrong_token_size() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();
    // wrong token size: 40 instead of 72
    svm.set_account(token_kp.pubkey(), token_account(program_id, vec![0u8; 40])).unwrap();

    let ix = mint_to_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), 1_000_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &mint_kp]);
    print_logs("mint_to_wrong_token_size", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_SIZE));
}

#[test]
fn test_mint_to_zero_amount() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 0))).unwrap();

    let ix = mint_to_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), 0);
    let result = send(&mut svm, ix, &payer, &[&payer, &mint_kp]);
    print_logs("mint_to_zero_amount", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_ZERO_AMOUNT));
}

#[test]
fn test_mint_to_token_overflow() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), u64::MAX))).unwrap();

    let ix = mint_to_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), 1);
    let result = send(&mut svm, ix, &payer, &[&payer, &mint_kp]);
    print_logs("mint_to_token_overflow", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_OVERFLOW));
}

#[test]
fn test_mint_to_wrong_ix_data_len() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, &mint_kp.pubkey())).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 0))).unwrap();

    // only disc byte, missing amount (should be 9 bytes)
    let ix = Instruction::new_with_bytes(
        program_id,
        &[IX_MINT_TO],
        vec![
            AccountMeta::new(mint_kp.pubkey(), true),
            AccountMeta::new(token_kp.pubkey(), false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer, &mint_kp]);
    print_logs("mint_to_wrong_ix_data_len", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_INVALID_IX));
}
