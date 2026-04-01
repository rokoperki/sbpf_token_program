# token_program

A minimal SPL-compatible token program written in raw sBPF assembly.

> **For educational purposes only. Do not deploy to mainnet.**

The goal is to explore how low compute units can go when writing Solana programs directly in sBPF assembly, bypassing all framework overhead (Anchor, Pinocchio, etc.).

---

## Benchmark

| instruction   | this program | Firedancer token.sBPF | SPL Token |
| ------------- | -----------: | --------------------: | --------: |
| transfer      |       118 CU |                 56 CU |   4736 CU |
| mint_to       |        89 CU |                     — |   4536 CU |
| burn          |       129 CU |                     — |   4753 CU |
| init_mint     |        56 CU |                     — |         — |
| init_token    |        95 CU |                     — |         — |
| set_authority |        89 CU |                     — |         — |
| close_account |        90 CU |                     — |         — |

SPL Token values measured on mainnet. Firedancer value from the [token.sBPF](https://github.com/firedancer-io/firedancer) source.

The transfer gap vs Firedancer (118 vs 56 CU) comes from additional safety checks: dup account detection, writable/signer flag validation, and account size verification. Firedancer skips several of these trusting the runtime.

**Binary size: 3008 bytes (~3 KB)**

---

## Account layout

### Mint account (40 bytes)

| offset | size | field           |
| ------ | ---- | --------------- |
| 0x00   | 32   | MA_AUTHORITY    |
| 0x20   | 8    | MA_TOTAL_MINTED |

### Token account (72 bytes)

| offset | size | field        |
| ------ | ---- | ------------ |
| 0x00   | 32   | TA_MINT      |
| 0x20   | 32   | TA_AUTHORITY |
| 0x40   | 8    | TA_BALANCE   |

---

## Instructions

| disc | name          | accounts                                          | ix data                                 |
| ---- | ------------- | ------------------------------------------------- | --------------------------------------- |
| —    | transfer      | [src writable, dst writable, authority signer]    | `amount: u64` (8 bytes, no disc)        |
| 0    | mint_to       | [mint writable+signer, token writable]            | `[0, amount: u64]` (9 bytes)            |
| 1    | set_authority | [token writable, authority signer]                | `[1, new_authority: pubkey]` (33 bytes) |
| 2    | burn          | [mint writable, token writable, authority signer] | `[2, amount: u64]` (9 bytes)            |
| 3    | init_mint     | [mint writable+signer]                            | `[3, authority: pubkey]` (33 bytes)     |
| 4    | init_token    | [token writable, mint]                            | `[4, authority: pubkey]` (33 bytes)     |
| 5    | close_account | [token writable, authority signer]                | `[5]` (1 byte)                          |

Transfer is detected by `data_len == 8` — no discriminator byte, same approach as Firedancer.
Mint authority = the mint keypair itself (must sign `mint_to`).

---

## Build

Requires Solana CLI tools with sBPF support.

```bash
cargo build-sbf
```

Output: `deploy/token_program.so`

---

## Tests

7 integration test suites using [litesvm](https://github.com/LiteSVM/litesvm), one per instruction. Each suite covers the happy path plus all error conditions: wrong account count, wrong sizes, missing signer/writable flag, arithmetic overflow/underflow, mint mismatch, authority mismatch, etc.

```bash
cargo test-sbf
```

Run the benchmark to print CU for all instructions:

```bash
cargo test-sbf bench_all -- --nocapture
```

---

## Tracing with agave-ledger-tool

You can trace execution of any instruction using a fixture JSON file that describes the accounts and instruction data.

```bash
agave-ledger-tool program run deploy/token_program.so \
  --ledger test-ledger \
  --mode interpreter \
  --input tests/fixtures/instruction_name.json \
  --trace traces/trace_instruction_name.txt
```

Fixture files live in `tests/fixtures/`:

| fixture              | instruction   |
| -------------------- | ------------- |
| `init_mint.json`     | init_mint     |
| `init_token.json`    | init_token    |
| `transfer.json`      | transfer      |
| `mint_to.json`       | mint_to       |
| `burn.json`          | burn          |
| `set_authority.json` | set_authority |
| `close_account.json` | close_account |

### Trace file format

The trace file starts with a `Frame 0` header, followed by one line per executed sBPF instruction:

```
Frame 0
    <ic> [r0, r1, r2, r3, r4, r5, r6, r7, r8, r9, r10]     <pc>: <disassembly>
```

Example:
```
Frame 0
    0 [0000000000000000, 0000000400000000, ...]     0: ldxdw r6, [r1+0x0]
    1 [0000000000000002, 0000000400000000, ...]     1: mov64 r7, r1
```

| field          | description                                                            |
| -------------- | ---------------------------------------------------------------------- |
| `ic`           | instruction counter — monotonically increasing, one per step executed  |
| `r0`–`r10`     | all 11 register values in hex at the point the instruction executes    |
| `pc`           | program counter — sBPF instruction index in the `.so`                  |
| `disassembly`  | human-readable sBPF mnemonic with operands                             |

Note: `pc` is not sequential when branches are taken — it jumps to the branch target. The `ic` always increments by 1 and is the real instruction count.

The trace is useful for counting exact instructions executed, verifying register state at each step, and spotting unexpected branches.
