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
const TA_BALANCE: usize = 0x40;

// ── Error codes (must match token_program.s) ──────────────────────────

const ERR_WRONG_ACCT_COUNT: u32 = 0x02;
const ERR_WRONG_ACCT_SIZE: u32 = 0x05;
const ERR_MINT_MISMATCH: u32 = 0x06;
const ERR_AUTHORITY_MISMATCH: u32 = 0x07;
const ERR_INSUFFICIENT_BALANCE: u32 = 0x08;
const ERR_OVERFLOW: u32 = 0x09;

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

fn transfer_ix(
    program_id: Pubkey,
    src: Pubkey,
    dst: Pubkey,
    authority: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &amount.to_le_bytes(),
        vec![
            AccountMeta::new(src, false),
            AccountMeta::new(dst, false),
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
fn test_transfer_happy_path() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let src_kp = Keypair::new();
    let dst_kp = Keypair::new();
    let authority = Keypair::new();
    let mint = Pubkey::new_unique();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(src_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(dst_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    let ix = transfer_ix(program_id, src_kp.pubkey(), dst_kp.pubkey(), authority.pubkey(), 500_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("transfer_happy_path", &result);
    result.unwrap();

    let src = svm.get_account(&src_kp.pubkey()).unwrap();
    let dst = svm.get_account(&dst_kp.pubkey()).unwrap();
    let src_bal = u64::from_le_bytes(src.data[TA_BALANCE..TA_BALANCE + 8].try_into().unwrap());
    let dst_bal = u64::from_le_bytes(dst.data[TA_BALANCE..TA_BALANCE + 8].try_into().unwrap());
    assert_eq!(src_bal, 500_000);
    assert_eq!(dst_bal, 500_000);
}

#[test]
fn test_transfer_full_balance() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let src_kp = Keypair::new();
    let dst_kp = Keypair::new();
    let authority = Keypair::new();
    let mint = Pubkey::new_unique();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(src_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(dst_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    let ix = transfer_ix(program_id, src_kp.pubkey(), dst_kp.pubkey(), authority.pubkey(), 1_000_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("transfer_full_balance", &result);
    result.unwrap();

    let src = svm.get_account(&src_kp.pubkey()).unwrap();
    let dst = svm.get_account(&dst_kp.pubkey()).unwrap();
    let src_bal = u64::from_le_bytes(src.data[TA_BALANCE..TA_BALANCE + 8].try_into().unwrap());
    let dst_bal = u64::from_le_bytes(dst.data[TA_BALANCE..TA_BALANCE + 8].try_into().unwrap());
    assert_eq!(src_bal, 0);
    assert_eq!(dst_bal, 1_000_000);
}

#[test]
fn test_transfer_wrong_authority() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let src_kp = Keypair::new();
    let dst_kp = Keypair::new();
    let authority = Keypair::new();
    let wrong_authority = Keypair::new();
    let mint = Pubkey::new_unique();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(src_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(dst_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    let ix = transfer_ix(program_id, src_kp.pubkey(), dst_kp.pubkey(), wrong_authority.pubkey(), 500_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &wrong_authority]);
    print_logs("transfer_wrong_authority", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_AUTHORITY_MISMATCH));
}

#[test]
fn test_transfer_mint_mismatch() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let src_kp = Keypair::new();
    let dst_kp = Keypair::new();
    let authority = Keypair::new();
    let mint_a = Pubkey::new_unique();
    let mint_b = Pubkey::new_unique();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(src_kp.pubkey(), token_account(program_id,
        token_data(&mint_a, &authority.pubkey(), 1_000_000))).unwrap();
    svm.set_account(dst_kp.pubkey(), token_account(program_id,
        token_data(&mint_b, &authority.pubkey(), 0))).unwrap();

    let ix = transfer_ix(program_id, src_kp.pubkey(), dst_kp.pubkey(), authority.pubkey(), 500_000);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("transfer_mint_mismatch", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_MINT_MISMATCH));
}

#[test]
fn test_transfer_insufficient_balance() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let src_kp = Keypair::new();
    let dst_kp = Keypair::new();
    let authority = Keypair::new();
    let mint = Pubkey::new_unique();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(src_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 100))).unwrap();
    svm.set_account(dst_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    let ix = transfer_ix(program_id, src_kp.pubkey(), dst_kp.pubkey(), authority.pubkey(), 101);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("transfer_insufficient_balance", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_INSUFFICIENT_BALANCE));
}

#[test]
fn test_transfer_dst_overflow() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let src_kp = Keypair::new();
    let dst_kp = Keypair::new();
    let authority = Keypair::new();
    let mint = Pubkey::new_unique();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(src_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 1))).unwrap();
    svm.set_account(dst_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), u64::MAX))).unwrap();

    let ix = transfer_ix(program_id, src_kp.pubkey(), dst_kp.pubkey(), authority.pubkey(), 1);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("transfer_dst_overflow", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_OVERFLOW));
}

#[test]
fn test_transfer_wrong_acct_count() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let src_kp = Keypair::new();
    let authority = Keypair::new();
    let mint = Pubkey::new_unique();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(src_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 1_000_000))).unwrap();

    // only 2 accounts — missing authority
    let ix = Instruction::new_with_bytes(
        program_id,
        &500_000u64.to_le_bytes(),
        vec![
            AccountMeta::new(src_kp.pubkey(), false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("transfer_wrong_acct_count", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_COUNT));
}

#[test]
fn test_transfer_wrong_acct_size() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let src_kp = Keypair::new();
    let dst_kp = Keypair::new();
    let authority = Keypair::new();
    let mint = Pubkey::new_unique();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    // src has wrong size
    svm.set_account(src_kp.pubkey(), token_account(program_id, vec![0u8; 40])).unwrap();
    svm.set_account(dst_kp.pubkey(), token_account(program_id,
        token_data(&mint, &authority.pubkey(), 0))).unwrap();

    let ix = transfer_ix(program_id, src_kp.pubkey(), dst_kp.pubkey(), authority.pubkey(), 1);
    let result = send(&mut svm, ix, &payer, &[&payer, &authority]);
    print_logs("transfer_wrong_acct_size", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_SIZE));
}
