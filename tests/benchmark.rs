use litesvm::LiteSVM;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

// ── Constants ─────────────────────────────────────────────────────────

const MINT_SZ: usize = 40;
const TOKEN_SZ: usize = 72;

const IX_MINT_TO: u8 = 0;
const IX_SET_AUTHORITY: u8 = 1;
const IX_BURN: u8 = 2;
const IX_INIT_MINT: u8 = 3;
const IX_INIT_TOKEN: u8 = 4;
const IX_CLOSE: u8 = 5;

// SPL Token CU reference values (mainnet, measured)
const SPL_TRANSFER_CU: u64 = 4736;
const SPL_MINT_TO_CU: u64 = 4536;
const SPL_BURN_CU: u64 = 4753;

const PROGRAM_ID_BYTES: [u8; 32] = [
    100, 90, 150, 68, 223, 75, 128, 162, 220, 199, 20, 12, 62, 211, 220, 12, 44, 32, 101, 98, 225,
    98, 2, 157, 66, 186, 18, 61, 84, 57, 104, 122,
];

// ── Helpers ───────────────────────────────────────────────────────────

fn setup() -> (LiteSVM, Pubkey) {
    let mut svm = LiteSVM::new();
    let program_id = Pubkey::from(PROGRAM_ID_BYTES);
    svm.add_program_from_file(program_id, "deploy/token_program.so")
        .expect("failed to load token_program.so — build it first");
    (svm, program_id)
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

fn send_and_cu(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) -> u64 {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        signers,
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx)
        .expect("transaction failed")
        .compute_units_consumed
}

// ── Benchmarks ────────────────────────────────────────────────────────

#[test]
fn bench_all() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let mint_kp = Keypair::new();
    let src_kp = Keypair::new();
    let dst_kp = Keypair::new();
    let authority = Keypair::new();
    let new_authority = Keypair::new();

    // ── init_mint ─────────────────────────────────────────────────────
    svm.set_account(
        mint_kp.pubkey(),
        make_account(program_id, vec![0u8; MINT_SZ]),
    )
    .unwrap();
    let mut ix_data = vec![IX_INIT_MINT];
    ix_data.extend_from_slice(&authority.pubkey().to_bytes());
    let cu_init_mint = send_and_cu(
        &mut svm,
        Instruction::new_with_bytes(
            program_id,
            &ix_data,
            vec![AccountMeta::new(mint_kp.pubkey(), true)],
        ),
        &payer,
        &[&payer, &mint_kp],
    );

    // ── init_token (src) ──────────────────────────────────────────────
    svm.set_account(
        src_kp.pubkey(),
        make_account(program_id, vec![0u8; TOKEN_SZ]),
    )
    .unwrap();
    let mut ix_data = vec![IX_INIT_TOKEN];
    ix_data.extend_from_slice(&authority.pubkey().to_bytes());
    let cu_init_token = send_and_cu(
        &mut svm,
        Instruction::new_with_bytes(
            program_id,
            &ix_data,
            vec![
                AccountMeta::new(src_kp.pubkey(), false),
                AccountMeta::new_readonly(mint_kp.pubkey(), false),
            ],
        ),
        &payer,
        &[&payer],
    );

    // ── init_token (dst) ──────────────────────────────────────────────
    svm.set_account(
        dst_kp.pubkey(),
        make_account(program_id, vec![0u8; TOKEN_SZ]),
    )
    .unwrap();
    let mut ix_data = vec![IX_INIT_TOKEN];
    ix_data.extend_from_slice(&authority.pubkey().to_bytes());
    send_and_cu(
        &mut svm,
        Instruction::new_with_bytes(
            program_id,
            &ix_data,
            vec![
                AccountMeta::new(dst_kp.pubkey(), false),
                AccountMeta::new_readonly(mint_kp.pubkey(), false),
            ],
        ),
        &payer,
        &[&payer],
    );

    // ── mint_to ───────────────────────────────────────────────────────
    let mut ix_data = vec![IX_MINT_TO];
    ix_data.extend_from_slice(&1_000_000u64.to_le_bytes());
    let cu_mint_to = send_and_cu(
        &mut svm,
        Instruction::new_with_bytes(
            program_id,
            &ix_data,
            vec![
                AccountMeta::new(mint_kp.pubkey(), true),
                AccountMeta::new(src_kp.pubkey(), false),
            ],
        ),
        &payer,
        &[&payer, &mint_kp],
    );

    // ── transfer ──────────────────────────────────────────────────────
    let cu_transfer = send_and_cu(
        &mut svm,
        Instruction::new_with_bytes(
            program_id,
            &400_000u64.to_le_bytes(),
            vec![
                AccountMeta::new(src_kp.pubkey(), false),
                AccountMeta::new(dst_kp.pubkey(), false),
                AccountMeta::new_readonly(authority.pubkey(), true),
            ],
        ),
        &payer,
        &[&payer, &authority],
    );

    // ── burn ──────────────────────────────────────────────────────────
    let mut ix_data = vec![IX_BURN];
    ix_data.extend_from_slice(&100_000u64.to_le_bytes());
    let cu_burn = send_and_cu(
        &mut svm,
        Instruction::new_with_bytes(
            program_id,
            &ix_data,
            vec![
                AccountMeta::new(mint_kp.pubkey(), false),
                AccountMeta::new(src_kp.pubkey(), false),
                AccountMeta::new_readonly(authority.pubkey(), true),
            ],
        ),
        &payer,
        &[&payer, &authority],
    );

    // ── set_authority ─────────────────────────────────────────────────
    let mut ix_data = vec![IX_SET_AUTHORITY];
    ix_data.extend_from_slice(&new_authority.pubkey().to_bytes());
    let cu_set_authority = send_and_cu(
        &mut svm,
        Instruction::new_with_bytes(
            program_id,
            &ix_data,
            vec![
                AccountMeta::new(dst_kp.pubkey(), false),
                AccountMeta::new_readonly(authority.pubkey(), true),
            ],
        ),
        &payer,
        &[&payer, &authority],
    );

    // ── close_account (drain dst first via burn) ──────────────────────
    // transfer remaining dst balance back to src so dst is zero
    let dst_acct = svm.get_account(&dst_kp.pubkey()).unwrap();
    let dst_bal = u64::from_le_bytes(dst_acct.data[64..72].try_into().unwrap());
    if dst_bal > 0 {
        // burn dst balance (need to update authority to new_authority first)
        let mut ix_data = vec![IX_BURN];
        ix_data.extend_from_slice(&dst_bal.to_le_bytes());
        send_and_cu(
            &mut svm,
            Instruction::new_with_bytes(
                program_id,
                &ix_data,
                vec![
                    AccountMeta::new(mint_kp.pubkey(), false),
                    AccountMeta::new(dst_kp.pubkey(), false),
                    AccountMeta::new_readonly(new_authority.pubkey(), true),
                ],
            ),
            &payer,
            &[&payer, &new_authority],
        );
    }
    let cu_close = send_and_cu(
        &mut svm,
        Instruction::new_with_bytes(
            program_id,
            &[IX_CLOSE],
            vec![
                AccountMeta::new(dst_kp.pubkey(), false),
                AccountMeta::new_readonly(new_authority.pubkey(), true),
            ],
        ),
        &payer,
        &[&payer, &new_authority],
    );

    // ── Report ────────────────────────────────────────────────────────
    println!();
    println!("┌─────────────────┬────────────┬────────────┬──────────┐");
    println!("│ instruction     │   tp CU    │   SPL CU   │  saving  │");
    println!("├─────────────────┼────────────┼────────────┼──────────┤");
    print_row("transfer", cu_transfer, Some(SPL_TRANSFER_CU));
    print_row("mint_to", cu_mint_to, Some(SPL_MINT_TO_CU));
    print_row("burn", cu_burn, Some(SPL_BURN_CU));
    print_row("init_mint", cu_init_mint, None);
    print_row("init_token", cu_init_token, None);
    print_row("set_authority", cu_set_authority, None);
    print_row("close_account", cu_close, None);
    println!("└─────────────────┴────────────┴────────────┴──────────┘");
    println!();
}

fn print_row(name: &str, ours: u64, spl: Option<u64>) {
    match spl {
        Some(spl_cu) => {
            let saving = spl_cu as i64 - ours as i64;
            let pct = saving * 100 / spl_cu as i64;
            println!(
                "  {:<15} │ {:>10} │ {:>10} │ {:>6}% ",
                name, ours, spl_cu, pct
            );
        }
        None => {
            println!("  {:<15} │ {:>10} │ {:>10} │ {:>8} ", name, ours, "—", "—");
        }
    }
}
