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
const MA_AUTHORITY: usize = 0x00;
const MA_TOTAL_MINTED: usize = 0x20;
const IX_INIT_MINT: u8 = 3;

// ── Error codes (must match token_program.s) ──────────────────────────

const ERR_WRONG_ACCT_COUNT: u32 = 0x02;
const ERR_NOT_SIGNER: u32 = 0x03;
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

fn make_ix(
    program_id: Pubkey,
    mint_pk: Pubkey,
    authority: Pubkey,
    data: Vec<u8>,
    mint_is_signer: bool,
    mint_is_writable: bool,
) -> Instruction {
    let meta = if mint_is_writable {
        AccountMeta::new(mint_pk, mint_is_signer)
    } else {
        AccountMeta::new_readonly(mint_pk, mint_is_signer)
    };
    Instruction::new_with_bytes(program_id, &data, vec![meta])
}

fn init_mint_data(authority: Pubkey) -> Vec<u8> {
    let mut data = vec![IX_INIT_MINT];
    data.extend_from_slice(&authority.to_bytes());
    data
}

fn mint_account(program_id: Pubkey, data: Vec<u8>) -> Account {
    Account {
        lamports: 1_141_440,
        data,
        owner: program_id,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

fn custom_err(code: u32) -> TransactionError {
    TransactionError::InstructionError(0, InstructionError::Custom(code))
}

fn print_logs(label: &str, result: &Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata>) {
    let logs = match result {
        Ok(meta) => &meta.logs,
        Err(e)   => &e.meta.logs,
    };
    println!("[{}]", label);
    for log in logs {
        println!("  {}", log);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[test]
fn test_init_mint_happy_path() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, vec![0u8; MINT_SZ])).unwrap();

    let ix = make_ix(program_id, mint_kp.pubkey(), authority.pubkey(),
                     init_mint_data(authority.pubkey()), true, true);
    let tx = Transaction::new_signed_with_payer(
        &[ix], Some(&payer.pubkey()), &[&payer, &mint_kp], svm.latest_blockhash(),
    );
    let result = svm.send_transaction(tx);
    print_logs("init_mint_happy_path", &result);
    result.unwrap();

    let acct = svm.get_account(&mint_kp.pubkey()).unwrap();
    // MA_AUTHORITY written correctly
    assert_eq!(&acct.data[MA_AUTHORITY..MA_AUTHORITY + 32], &authority.pubkey().to_bytes());
    // MA_TOTAL_MINTED stays zero
    assert_eq!(&acct.data[MA_TOTAL_MINTED..MA_TOTAL_MINTED + 8], &[0u8; 8]);
}

#[test]
fn test_init_mint_wrong_acct_count() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let authority = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    // 0 accounts
    let ix = Instruction::new_with_bytes(program_id, &init_mint_data(authority.pubkey()), vec![]);
    let tx = Transaction::new_signed_with_payer(
        &[ix], Some(&payer.pubkey()), &[&payer], svm.latest_blockhash(),
    );
    let result = svm.send_transaction(tx);
    print_logs("init_mint_wrong_acct_count", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_COUNT));
}

#[test]
fn test_init_mint_not_signer() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, vec![0u8; MINT_SZ])).unwrap();

    // mint is writable but NOT signer
    let ix = make_ix(program_id, mint_kp.pubkey(), authority.pubkey(),
                     init_mint_data(authority.pubkey()), false, true);
    let tx = Transaction::new_signed_with_payer(
        &[ix], Some(&payer.pubkey()), &[&payer], svm.latest_blockhash(),
    );
    let result = svm.send_transaction(tx);
    print_logs("init_mint_not_signer", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_SIGNER));
}

#[test]
fn test_init_mint_not_writable() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, vec![0u8; MINT_SZ])).unwrap();

    // mint is signer but NOT writable
    let ix = make_ix(program_id, mint_kp.pubkey(), authority.pubkey(),
                     init_mint_data(authority.pubkey()), true, false);
    let tx = Transaction::new_signed_with_payer(
        &[ix], Some(&payer.pubkey()), &[&payer, &mint_kp], svm.latest_blockhash(),
    );
    let result = svm.send_transaction(tx);
    print_logs("init_mint_not_writable", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_WRITABLE));
}

#[test]
fn test_init_mint_wrong_acct_size() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    // wrong size: 8 bytes instead of 40
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, vec![0u8; 8])).unwrap();

    let ix = make_ix(program_id, mint_kp.pubkey(), authority.pubkey(),
                     init_mint_data(authority.pubkey()), true, true);
    let tx = Transaction::new_signed_with_payer(
        &[ix], Some(&payer.pubkey()), &[&payer, &mint_kp], svm.latest_blockhash(),
    );
    let result = svm.send_transaction(tx);
    print_logs("init_mint_wrong_acct_size", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_SIZE));
}

#[test]
fn test_init_mint_wrong_ix_data_len() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, vec![0u8; MINT_SZ])).unwrap();

    // only disc byte, missing authority (should be 33 bytes)
    let ix = make_ix(program_id, mint_kp.pubkey(), Pubkey::default(),
                     vec![IX_INIT_MINT], true, true);
    let tx = Transaction::new_signed_with_payer(
        &[ix], Some(&payer.pubkey()), &[&payer, &mint_kp], svm.latest_blockhash(),
    );
    let result = svm.send_transaction(tx);
    print_logs("init_mint_wrong_ix_data_len", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_INVALID_IX));
}

#[test]
fn test_init_mint_already_initialized() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    let mint_kp = Keypair::new();
    let authority = Keypair::new();

    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    // MA_TOTAL_MINTED (bytes 32..40) is nonzero → already initialized
    let mut data = vec![0u8; MINT_SZ];
    data[MA_TOTAL_MINTED] = 1;
    svm.set_account(mint_kp.pubkey(), mint_account(program_id, data)).unwrap();

    let ix = make_ix(program_id, mint_kp.pubkey(), authority.pubkey(),
                     init_mint_data(authority.pubkey()), true, true);
    let tx = Transaction::new_signed_with_payer(
        &[ix], Some(&payer.pubkey()), &[&payer, &mint_kp], svm.latest_blockhash(),
    );
    let result = svm.send_transaction(tx);
    print_logs("init_mint_already_initialized", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_ALREADY_INITIALIZED));
}
