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
const TA_MINT: usize = 0x00;
const TA_AUTHORITY: usize = 0x20;
const TA_BALANCE: usize = 0x40;
const IX_INIT_TOKEN: u8 = 4;

// ── Error codes (must match token_program.s) ──────────────────────────

const ERR_WRONG_ACCT_COUNT: u32 = 0x02;
const ERR_NOT_WRITABLE: u32 = 0x04;
const ERR_WRONG_ACCT_SIZE: u32 = 0x05;
const ERR_INVALID_IX: u32 = 0x01;
const ERR_ALREADY_INITIALIZED: u32 = 0x0B;

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

fn token_account(program_id: Pubkey, data: Vec<u8>) -> Account {
    Account {
        lamports: 1_141_440,
        data,
        owner: program_id,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

fn mint_account(program_id: Pubkey) -> Account {
    // initialized mint: MA_AUTHORITY (32b) + MA_TOTAL_MINTED=0 (8b)
    Account {
        lamports: 1_141_440,
        data: vec![0u8; MINT_SZ],
        owner: program_id,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

fn init_token_data(authority: Pubkey) -> Vec<u8> {
    let mut data = vec![IX_INIT_TOKEN];
    data.extend_from_slice(&authority.to_bytes());
    data
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
fn test_init_token_happy_path() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id, vec![0u8; TOKEN_SZ])).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id)).unwrap();

    let ix = Instruction::new_with_bytes(
        program_id,
        &init_token_data(authority.pubkey()),
        vec![
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(mint_kp.pubkey(), false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_token_happy_path", &result);
    result.unwrap();

    let acct = svm.get_account(&token_kp.pubkey()).unwrap();
    assert_eq!(&acct.data[TA_MINT..TA_MINT + 32],       &mint_kp.pubkey().to_bytes());
    assert_eq!(&acct.data[TA_AUTHORITY..TA_AUTHORITY + 32], &authority.pubkey().to_bytes());
    assert_eq!(&acct.data[TA_BALANCE..TA_BALANCE + 8],   &[0u8; 8]);
}

#[test]
fn test_init_token_wrong_acct_count() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id, vec![0u8; TOKEN_SZ])).unwrap();

    // only 1 account — missing mint
    let ix = Instruction::new_with_bytes(
        program_id,
        &init_token_data(authority.pubkey()),
        vec![AccountMeta::new(token_kp.pubkey(), false)],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_token_wrong_acct_count", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_COUNT));
}

#[test]
fn test_init_token_not_writable() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id, vec![0u8; TOKEN_SZ])).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id)).unwrap();

    // token account is readonly
    let ix = Instruction::new_with_bytes(
        program_id,
        &init_token_data(authority.pubkey()),
        vec![
            AccountMeta::new_readonly(token_kp.pubkey(), false),
            AccountMeta::new_readonly(mint_kp.pubkey(), false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_token_not_writable", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_WRITABLE));
}

#[test]
fn test_init_token_wrong_token_size() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    // wrong size: 40 instead of 72
    svm.set_account(token_kp.pubkey(), token_account(program_id, vec![0u8; 40])).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id)).unwrap();

    let ix = Instruction::new_with_bytes(
        program_id,
        &init_token_data(authority.pubkey()),
        vec![
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(mint_kp.pubkey(), false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_token_wrong_token_size", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_SIZE));
}

#[test]
fn test_init_token_wrong_mint_size() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id, vec![0u8; TOKEN_SZ])).unwrap();
    // wrong mint size: 8 instead of 40
    svm.set_account(mint_kp.pubkey(), token_account(program_id, vec![0u8; 8])).unwrap();

    let ix = Instruction::new_with_bytes(
        program_id,
        &init_token_data(authority.pubkey()),
        vec![
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(mint_kp.pubkey(), false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_token_wrong_mint_size", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_SIZE));
}

#[test]
fn test_init_token_wrong_ix_data_len() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint_kp = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id, vec![0u8; TOKEN_SZ])).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id)).unwrap();

    // only disc byte, missing authority (should be 33 bytes)
    let ix = Instruction::new_with_bytes(
        program_id,
        &[IX_INIT_TOKEN],
        vec![
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(mint_kp.pubkey(), false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_token_wrong_ix_data_len", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_INVALID_IX));
}

#[test]
fn test_init_token_already_initialized() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id)).unwrap();

    // TA_MINT already nonzero — token account already initialized
    let mut data = vec![0u8; TOKEN_SZ];
    data[TA_MINT] = 1;
    svm.set_account(token_kp.pubkey(), token_account(program_id, data)).unwrap();

    let ix = Instruction::new_with_bytes(
        program_id,
        &init_token_data(authority.pubkey()),
        vec![
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(mint_kp.pubkey(), false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_token_already_initialized", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_ALREADY_INITIALIZED));
}
