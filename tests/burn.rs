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
const IX_BURN: u8 = 2;

// ── Error codes (must match token_program.s) ──────────────────────────

const ERR_INVALID_IX: u32 = 0x01;
const ERR_WRONG_ACCT_COUNT: u32 = 0x02;
const ERR_NOT_SIGNER: u32 = 0x03;
const ERR_NOT_WRITABLE: u32 = 0x04;
const ERR_WRONG_ACCT_SIZE: u32 = 0x05;
const ERR_MINT_MISMATCH: u32 = 0x06;
const ERR_AUTHORITY_MISMATCH: u32 = 0x07;
const ERR_INSUFFICIENT_BALANCE: u32 = 0x08;
const ERR_ZERO_AMOUNT: u32 = 0x0a;
const ERR_SUPPLY_UNDERFLOW: u32 = 0x0c;

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

fn mint_data(authority: &Pubkey, total_minted: u64) -> Vec<u8> {
    let mut d = vec![0u8; MINT_SZ];
    d[0..32].copy_from_slice(&authority.to_bytes());
    d[32..40].copy_from_slice(&total_minted.to_le_bytes());
    d
}

fn token_data(mint: &Pubkey, authority: &Pubkey, balance: u64) -> Vec<u8> {
    let mut d = vec![0u8; TOKEN_SZ];
    d[0..32].copy_from_slice(&mint.to_bytes());
    d[32..64].copy_from_slice(&authority.to_bytes());
    d[64..72].copy_from_slice(&balance.to_le_bytes());
    d
}

fn make_account(program_id: Pubkey, data: Vec<u8>) -> Account {
    Account {
        lamports: 1_141_440,
        data,
        owner: program_id,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

fn burn_ix(
    program_id: Pubkey,
    mint: Pubkey,
    token: Pubkey,
    authority: Pubkey,
    amount: u64,
) -> Instruction {
    let mut data = vec![IX_BURN];
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction::new_with_bytes(
        program_id,
        &data,
        vec![
            AccountMeta::new(mint, false),
            AccountMeta::new(token, false),
            AccountMeta::new_readonly(authority, true),
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
fn test_burn_happy_path() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    let ix = burn_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), authority.pubkey(), 400_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_happy_path", &result);
    result.unwrap();

    let mint = svm.get_account(&mint_kp.pubkey()).unwrap();
    let token = svm.get_account(&token_kp.pubkey()).unwrap();
    let total_minted = u64::from_le_bytes(mint.data[MA_TOTAL_MINTED..MA_TOTAL_MINTED + 8].try_into().unwrap());
    let balance = u64::from_le_bytes(token.data[TA_BALANCE..TA_BALANCE + 8].try_into().unwrap());
    assert_eq!(total_minted, 600_000);
    assert_eq!(balance, 600_000);
}

#[test]
fn test_burn_full_balance() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    let ix = burn_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), authority.pubkey(), 1_000_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_full_balance", &result);
    result.unwrap();

    let mint = svm.get_account(&mint_kp.pubkey()).unwrap();
    let token = svm.get_account(&token_kp.pubkey()).unwrap();
    let total_minted = u64::from_le_bytes(mint.data[MA_TOTAL_MINTED..MA_TOTAL_MINTED + 8].try_into().unwrap());
    let balance = u64::from_le_bytes(token.data[TA_BALANCE..TA_BALANCE + 8].try_into().unwrap());
    assert_eq!(total_minted, 0);
    assert_eq!(balance, 0);
}

#[test]
fn test_burn_wrong_authority() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();
    let wrong_authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    let ix = burn_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), wrong_authority.pubkey(), 400_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &wrong_authority]);
    print_logs("burn_wrong_authority", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_AUTHORITY_MISMATCH));
}

#[test]
fn test_burn_mint_mismatch() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let wrong_mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(wrong_mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    // token points to mint_kp, but we pass wrong_mint_kp
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    let ix = burn_ix(program_id, wrong_mint_kp.pubkey(), token_kp.pubkey(), authority.pubkey(), 400_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_mint_mismatch", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_MINT_MISMATCH));
}

#[test]
fn test_burn_insufficient_balance() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 100))).unwrap();

    let ix = burn_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), authority.pubkey(), 101);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_insufficient_balance", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_INSUFFICIENT_BALANCE));
}

#[test]
fn test_burn_supply_underflow() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    // total_minted less than token balance (shouldn't happen in practice but must be handled)
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 50))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    let ix = burn_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), authority.pubkey(), 100);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_supply_underflow", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_SUPPLY_UNDERFLOW));
}

#[test]
fn test_burn_zero_amount() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    let ix = burn_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), authority.pubkey(), 0);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_zero_amount", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_ZERO_AMOUNT));
}

#[test]
fn test_burn_wrong_acct_count() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    // only 2 accounts — missing authority
    let mut data = vec![IX_BURN];
    data.extend_from_slice(&400_000u64.to_le_bytes());
    let ix = Instruction::new_with_bytes(
        program_id,
        &data,
        vec![
            AccountMeta::new(mint_kp.pubkey(), false),
            AccountMeta::new(token_kp.pubkey(), false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("burn_wrong_acct_count", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_COUNT));
}

#[test]
fn test_burn_not_signer() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    // authority not a signer
    let mut data = vec![IX_BURN];
    data.extend_from_slice(&400_000u64.to_le_bytes());
    let ix = Instruction::new_with_bytes(
        program_id,
        &data,
        vec![
            AccountMeta::new(mint_kp.pubkey(), false),
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(authority.pubkey(), false), // not signer
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("burn_not_signer", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_SIGNER));
}

#[test]
fn test_burn_mint_not_writable() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    let mut data = vec![IX_BURN];
    data.extend_from_slice(&400_000u64.to_le_bytes());
    let ix = Instruction::new_with_bytes(
        program_id,
        &data,
        vec![
            AccountMeta::new_readonly(mint_kp.pubkey(), false), // not writable
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_mint_not_writable", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_WRITABLE));
}

#[test]
fn test_burn_wrong_mint_size() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id, vec![0u8; 8])).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    let ix = burn_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), authority.pubkey(), 400_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_wrong_mint_size", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_SIZE));
}

#[test]
fn test_burn_wrong_token_size() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id, vec![0u8; 40])).unwrap();

    let ix = burn_ix(program_id, mint_kp.pubkey(), token_kp.pubkey(), authority.pubkey(), 400_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_wrong_token_size", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_SIZE));
}

#[test]
fn test_burn_wrong_ix_data_len() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), make_account(program_id,
        mint_data(&authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(token_kp.pubkey(), make_account(program_id,
        token_data(&mint_kp.pubkey(), &authority.pubkey(), 1_000_000))).unwrap();

    // only disc byte, missing amount (should be 9 bytes)
    let ix = Instruction::new_with_bytes(
        program_id,
        &[IX_BURN],
        vec![
            AccountMeta::new(mint_kp.pubkey(), false),
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("burn_wrong_ix_data_len", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_INVALID_IX));
}
