use litesvm::LiteSVM;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction, InstructionError},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::{Transaction, TransactionError},
};

// ── Constants (must match offsets.py) ────────────────────────────────

const TOKEN_SZ: usize = 72;
const IX_CLOSE: u8 = 5;

// ── Error codes (must match token_program.s) ──────────────────────────

const ERR_INVALID_IX: u32 = 0x01;
const ERR_WRONG_ACCT_COUNT: u32 = 0x02;
const ERR_NOT_SIGNER: u32 = 0x03;
const ERR_NOT_WRITABLE: u32 = 0x04;
const ERR_WRONG_ACCT_SIZE: u32 = 0x05;
const ERR_AUTHORITY_MISMATCH: u32 = 0x07;
const ERR_NONZERO_BALANCE: u32 = 0x0d;

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

fn close_ix(program_id: Pubkey, token: Pubkey, authority: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &[IX_CLOSE],
        vec![
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
fn test_close_account_happy_path() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint = Pubkey::new_unique();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    let ix = close_ix(program_id, token_kp.pubkey(), authority.pubkey());
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("close_account_happy_path", &result);
    result.unwrap();

    let acct = svm.get_account(&token_kp.pubkey()).unwrap();
    assert_eq!(acct.data, vec![0u8; TOKEN_SZ]);
}

#[test]
fn test_close_account_nonzero_balance() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint = Pubkey::new_unique();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 1))).unwrap();

    let ix = close_ix(program_id, token_kp.pubkey(), authority.pubkey());
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("close_account_nonzero_balance", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NONZERO_BALANCE));
}

#[test]
fn test_close_account_wrong_authority() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint = Pubkey::new_unique();
    let authority = Keypair::new();
    let wrong_authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    let ix = close_ix(program_id, token_kp.pubkey(), wrong_authority.pubkey());
    let result = send(&mut svm, ix, &payer, &[&payer, &wrong_authority]);
    print_logs("close_account_wrong_authority", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_AUTHORITY_MISMATCH));
}

#[test]
fn test_close_account_not_signer() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint = Pubkey::new_unique();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    let ix = Instruction::new_with_bytes(
        program_id,
        &[IX_CLOSE],
        vec![
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(authority.pubkey(), false), // not signer
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("close_account_not_signer", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_SIGNER));
}

#[test]
fn test_close_account_not_writable() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint = Pubkey::new_unique();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    let ix = Instruction::new_with_bytes(
        program_id,
        &[IX_CLOSE],
        vec![
            AccountMeta::new_readonly(token_kp.pubkey(), false), // not writable
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("close_account_not_writable", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_WRITABLE));
}

#[test]
fn test_close_account_wrong_acct_count() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint = Pubkey::new_unique();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    // only 1 account — missing authority
    let ix = Instruction::new_with_bytes(
        program_id,
        &[IX_CLOSE],
        vec![AccountMeta::new(token_kp.pubkey(), false)],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("close_account_wrong_acct_count", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_COUNT));
}

#[test]
fn test_close_account_wrong_token_size() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id, vec![0u8; 40])).unwrap();

    let ix = close_ix(program_id, token_kp.pubkey(), authority.pubkey());
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("close_account_wrong_token_size", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_SIZE));
}

#[test]
fn test_close_account_wrong_ix_data_len() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let token_kp = Keypair::new();
    let mint = Pubkey::new_unique();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(token_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    // extra byte after disc
    let ix = Instruction::new_with_bytes(
        program_id,
        &[IX_CLOSE, 0],
        vec![
            AccountMeta::new(token_kp.pubkey(), false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("close_account_wrong_ix_data_len", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_INVALID_IX));
}
