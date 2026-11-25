---
source: antigravity
surface: "cli"
session_id: "1916eb2c-5f13-4a6b-b35c-a9cd3c2c9a31"
title: "Implement Issue Number 5"
date: "2026-11-24"
message_count: 23
project_directory: "/home/neithanmo/Documents/Projects/transaction-engine"
---

# Implement Issue Number 5

## User [21:23]

/implement #5

## Assistant [21:26]

Starting work on **#5: Engine: Transactions (deposits, withdrawals, duplicate ids, locking)**, working in TDD vertical slices at Seam 2 (`Engine::apply` / `Engine::accounts`).

Running the first test (Deposit credits Available and Total, Accounts read in client order) to verify it fails before implementation.

## Assistant [21:26]

Slice 1 is green: Deposits credit `Available` and `Total`, store applied Deposits as `Settled`, and `Engine::accounts()` yields Accounts in ascending `ClientId` order. Next, adding failing tests for Withdrawals (valid debits and over-Available Rejections).

## Assistant [21:27]

Slice 2 is green: Withdrawals debit `Available` and `Total`, over-Available Withdrawals return `Rejection::InsufficientFunds` without changing balances, and a Rejected Withdrawal for an unknown Client still creates a zero Account. Next, adding failing tests for duplicate Transaction ids.

## Assistant [21:27]

Slice 3 is green: duplicate Transaction ids are Rejected before Account creation, covering Deposit-after-Deposit, Deposit-after-Rejected-Withdrawal, and duplicates for a new Client. Next, adding failing tests for Locked Accounts (rejecting Withdrawals while accepting Deposits).

## Assistant [21:28]

Slice 4 is green: a Locked Account rejects Withdrawals with `Rejection::AccountLocked` (using up their Transaction id and taking precedence over insufficient funds) while still accepting Deposits. Next, adding tests for overflow on `Available` and computed `Total`, plus non-positive Transaction amounts.

## Assistant [21:28]

Slice 5 is green: overflow on `Available` and on computed `Total` both return `Rejection::Overflow` without modifying balances (while keeping the Transaction id used up), and non-positive amounts return `Rejection::NonPositiveAmount` without side effects. All 36 tests and `cargo clippy --all-targets -- -D warnings` are clean. Next, running the `code-review` skill on the changes.

## Assistant [21:29]

Both Standards and Spec review subagents are running in parallel. Waiting for their reports.

## Assistant [21:31]

Standards review received; waiting for the Spec review report.

## Assistant [21:32]

The two-axis code review finished and flagged four concrete improvements to make before committing:

1. **Remove `Rejection::NonPositiveAmount`** (flagged by both Standards and Spec): checking positivity inside `Engine::apply` before step 1 conflated `Malformed` input validation with `Rejection` and conflicted with ADR 0001 / Issue #5's exact check order. Replacing with a `debug_assert!` precondition (positivity belongs to the input adapter in #7).
2. **Avoid glossary term `tx`** (Standards): rename `tx` to `transaction` / `transaction_id` in `apply` and test helpers.
3. **Move balance mutations onto `Account`** (Standards — Feature Envy & Duplicated Code): extract `Account::apply_deposit`, `Account::apply_withdrawal`, and a shared `Account::set_available` invariant check so `Account` owns its own `available + held` invariant.
4. **Trim unrequested `Engine::account` public method** (Spec): keep `Engine`'s public surface to `new`, `apply`, and `accounts` (with tests querying through `accounts()`).

Applying these refinements now and re-running the full test suite and clippy.

## Assistant [21:33]

All 35 tests pass and `cargo clippy --all-targets -- -D warnings` is clean after applying the code-review fixes. Checking formatting and committing `src/engine.rs` and `src/lib.rs` to the current branch.

## Assistant [21:34]

Implemented **#5: Engine: Transactions (deposits, withdrawals, duplicate ids, locking)** and committed it to `main` (`0d234aa`).

### Changes

- [src/engine.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/engine.rs):
  - `Engine`: holds a global `HashSet<TransactionId>` of seen Transaction ids and a `BTreeMap<ClientId, Account>` iterated in ascending `ClientId` order via `Engine::accounts()`. Exposes `Engine::new()`, `Engine::apply(Record) -> Result<(), Rejection>`, and `Engine::accounts()`.
  - `Account`: holds `available`, `held`, `locked`, and per-Account `HashMap<TransactionId, Deposit>` (amount + `DepositState::Settled`, per [0001-only-deposits-are-disputable.md](file:///home/neithanmo/Documents/Projects/transaction-engine/docs/adr/0001-only-deposits-are-disputable.md)). `Account::total()` computes `available + held` rather than storing it, and `Account::set_available()` enforces overflow checks on both `available` and derived `total`.
  - `Rejection`: `DuplicateTransactionId`, `InsufficientFunds`, `AccountLocked`, and `Overflow` (expected domain outcomes, separate from input-format errors).
  - Follows the exact Transaction check order: record Transaction id (reject duplicates before Account creation) → create Account if missing → apply Locked / Available / overflow rules → store applied Deposits as `Settled`. `Record::DisputeAction` is left as `todo!()` for #6.
  - 11 unit tests at the `Engine` seam covering Deposits, client-order iteration, Withdrawals, over-Available Rejections (including unknown Client zero-Account creation), duplicate Transaction ids (after Deposit, after Rejected Withdrawal, and for a new Client), Locked Accounts, and overflow on both `Available` and computed `Total`.
- [src/lib.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/lib.rs): exports `pub mod engine;` and re-exports `Account`, `Engine`, and `Rejection`.

### Code Review Summary

Both Standards and Spec reviewers ran in parallel and flagged findings that were resolved before committing:

- **Standards (resolved)**: removed a pre-step `Rejection::NonPositiveAmount` check that conflicted with ADR 0001's id-first rule; renamed `tx` identifiers to `transaction` / `transaction_id` per [GLOSSARY.md](file:///home/neithanmo/Documents/Projects/transaction-engine/GLOSSARY.md); moved `apply_deposit`, `apply_withdrawal`, and shared `set_available` invariant checks onto `Account` to eliminate Feature Envy and duplicated overflow checks.
- **Spec (resolved)**: removed `Rejection::NonPositiveAmount` (keeping `Rejection` free of bad-input validation) and removed an unrequested public `Engine::account()` lookup method so the public interface matches #5 exactly.

### Verification

- `cargo test`: **35 passed, 0 failed** (24 `amount` + 11 `engine` tests)
- `cargo clippy --all-targets -- -D warnings`: clean
- `rustfmt --edition 2024 --check src/engine.rs`: clean

## User [21:41]

/retro

## User [21:45]

/retro

## Assistant [21:45]

Here is the retrospective for this `/implement #5` session, ordered by severity.

---

### 1. [High] Automated checks — No CI or pre-commit guardrail exists

**What happened:** The repo has no `.github/workflows/` CI workflow and no active `.git/hooks/pre-commit` hook. When checking formatting before committing #5, `cargo fmt --check` failed on pre-existing unformatted lines in [src/amount.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/amount.rs) (requiring a separate `ci: cargo fmt` commit `d919be7` afterwards), and running `rustfmt --check src/lib.rs` still walked into `src/amount.rs` via `pub mod amount;`, costing extra shell turns. `cargo clippy --all-targets -- -D warnings` and `cargo test` also rely solely on the agent remembering to run them.

**Suggested fix:**

- Add `.github/workflows/ci.yml` running:

  ```sh
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  cargo test
  ```

- Optionally add a repo-tracked `.githooks/pre-commit` (enabled via `git config core.hooksPath .githooks`) running `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.

---

### 2. [Medium-High] Automated checks — Enforce banned glossary identifiers with Clippy instead of review prose

**What happened:** Even after reading [GLOSSARY.md](file:///home/neithanmo/Documents/Projects/transaction-engine/GLOSSARY.md) (`_Avoid_: Tx`, `_Avoid_: ... tx`), the implementation agent named a match binding `tx` and two test parameters `tx: u32` in [src/engine.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/engine.rs). The Standards review subagent caught it, forcing a rewrite pass.

**Why a deterministic check beats a review rule:** Banning specific variable/parameter names is mechanical. Clippy already has a built-in, warn-by-default `clippy::disallowed_names` lint configured via `clippy.toml` — and it only checks Rust binding identifiers, so CSV string literals like `"tx"` in the upcoming adapter (#7) are unaffected.

**Suggested fix:**

- Add `clippy.toml` at the repo root:

  ```toml
  disallowed-names = ["..", "tx", "wallet", "customer", "user", "row"]
  ```

  Paired with CI/hook `cargo clippy --all-targets -- -D warnings`, this catches glossary drift immediately during implementation without spending review attention on it.

---

### 3. [Medium] Domain / Spec clarity — Ambiguous ownership of `Transaction.amount > 0` caused the main rework

**What happened:** The largest rework in the session came from adding `Rejection::NonPositiveAmount` and a step-0 check in `Engine::apply`, which both reviewers flagged as contradicting [0001-only-deposits-are-disputable.md](file:///home/neithanmo/Documents/Projects/transaction-engine/docs/adr/0001-only-deposits-are-disputable.md) and Issue #5 (*"The enum must not be an error for bad input"*).

**Root cause in the repo docs/types:**

- [README.md](file:///home/neithanmo/Documents/Projects/transaction-engine/README.md#L49-L53) says *"These Records are **Rejected**: ... a zero or negative amount, or a Deposit or Withdrawal with no amount"*, using the glossary term **Rejected** for cases that its own [Input and errors table](file:///home/neithanmo/Documents/Projects/transaction-engine/README.md#L61-L65) and Issue #1 classify as **Malformed**.
- [src/record.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/record.rs#L65-L66) says `/// Positivity is a business rule checked by the caller.` without specifying *which* boundary (the CSV adapter, not `Engine`), while leaving `Transaction.amount` as an unconstrained `Amount`.

**Suggested fix:**

- **Preferred (type-level):** Introduce a `PositiveAmount` newtype (or fallible `Transaction::new` constructor) in [src/record.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/record.rs), mirroring how `Record` already makes an amount-less `Transaction` unrepresentable. That eliminates the ambiguity permanently across #5, #6, and #7.
- **Doc alignment:** Reword [README.md](file:///home/neithanmo/Documents/Projects/transaction-engine/README.md#L49-L53) so Malformed inputs (more than four significant decimals, zero/negative amount, missing amount) are not grouped under the glossary term **Rejected** alongside overflow.

---

### 4. [Medium-Low] Tool economy — Redundant `gh issue view` calls and truncated terminal output

**What happened:**

- Calling bare `gh issue view 1` produced ~30 KB of padded terminal output that truncated to disk, requiring an extra `read_file` turn.
- During code review, the Spec subagent (`f7aef252`) re-ran `gh issue list` and `gh issue view 1 && gh issue view 7` (truncating again and spending 3 extra turns / ~45K input tokens) even though Issue #5 and the relevant excerpts of #1 were already embedded in its prompt.

**Suggested fix:**

- In [docs/agents/issue-tracker.md](file:///home/neithanmo/Documents/Projects/transaction-engine/docs/agents/issue-tracker.md#L8), add `--jq` to the read recipe (e.g., `gh issue view <n> --json title,body,labels,comments --jq '"\(.title)\n\n\(.body)"'`) so `gh` outputs compact markdown without terminal margin padding or truncation.
- In [code-review/SKILL.md](file:///home/neithanmo/.agents/skills/code-review/SKILL.md#L68-L72), state explicitly in the Spec subagent brief: *"Use the spec provided below; do not re-fetch issues from the tracker."*

---

### 5. [Low] Skill alignment — `code-review` assumes committed work while `/implement` calls it pre-commit

**What happened:** `/implement` runs `code-review` *before* committing, when changes are unstaged/untracked (`src/engine.rs` was untracked). [code-review/SKILL.md](file:///home/neithanmo/.agents/skills/code-review/SKILL.md#L17-L23) only specifies `git diff <fixed-point>...HEAD` and fails on an empty diff, forcing a manual workaround listing modified and untracked files by path.

**Suggested fix:**

- Add a one-line WIP branch to [code-review/SKILL.md](file:///home/neithanmo/.agents/skills/code-review/SKILL.md#L17-L23): when reviewing uncommitted changes (e.g., from `/implement`), capture `git diff HEAD` plus untracked files from `git status --porcelain` (or run `git add -N` before diffing).

---

### 6. [Low] Workspace hygiene — Untracked `.claude/` noise in every `git status`

**What happened:** `.claude/` appeared as untracked in every `git status` call across the parent session and both review subagents because [`.gitignore`](file:///home/neithanmo/Documents/Projects/transaction-engine/.gitignore) only ignores `/target` and `*.pdf`.

**Suggested fix:**

- Add `.claude/` to `.git/info/exclude` (keeps the repo's `.gitignore` tool-agnostic while eliminating the noise locally).

---

*(Checked and clean: [AGENTS.md](file:///home/neithanmo/Documents/Projects/transaction-engine/AGENTS.md) is 9 lines with sharp navigation pointers and no no-ops; no oversized global steering files exist.)*

Let me know which of these you'd like me to apply.

## User [21:57]

lets add 2, regarding 3 that "contradiction" would depend of the point of view, and negative or non-present amount in deposit or withdrawals classifies that transsaction as malformed, so the transaction is rejected, that means ignored and we continue with the next entry. what is the issue in there?

## Assistant [21:57]

Added [clippy.toml](file:///home/neithanmo/Documents/Projects/transaction-engine/clippy.toml) using `clippy::disallowed_names` for the unambiguous avoided terms from [GLOSSARY.md](file:///home/neithanmo/Documents/Projects/transaction-engine/GLOSSARY.md) (`tx`, `wallet`, `customer`, `user`, `claim`, `reversal`, `frozen`), while leaving string literals like the CSV `"tx"` header unaffected. `cargo clippy --all-targets -- -D warnings` passes cleanly. (Left uncommitted so it doesn't collide with the other uncommitted doc edits currently in your working tree.)

---

### Regarding #3: What was the issue?

There is **no contradiction in the behavior you described** — a negative, zero, or missing amount on a Deposit or Withdrawal is indeed **Malformed**, skipped, and processing continues with the next Record.

The issue was only **which layer owns that check**, because two places in the repo blur the line between **Malformed** (CSV adapter) and **Rejected** (`Engine`):

1. **Different side effects in the spec** ([README.md](file:///home/neithanmo/Documents/Projects/transaction-engine/README.md#L61-L65) and [0001-only-deposits-are-disputable.md](file:///home/neithanmo/Documents/Projects/transaction-engine/docs/adr/0001-only-deposits-are-disputable.md)):
   - **Malformed** (rejected at the CSV adapter, before `Engine`): logs a **stderr warning**, creates **no Account**, and **does not use up the Transaction id**.
   - **Rejected** (returned by `Engine::apply` as `Err(Rejection)`): skipped **without** a stderr warning, and for any non-duplicate Transaction it **still creates the Account and uses up the Transaction id**.
2. **Where the agent got tripped up:**
   - [README.md](file:///home/neithanmo/Documents/Projects/transaction-engine/README.md#L49-L53) writes *"These Records are **Rejected**: … a zero or negative amount, or a Deposit or Withdrawal with no amount; a Record that would overflow…"*, using the capitalized glossary word **Rejected** for cases that actually belong to **Malformed** (plus overflow, which really is `Engine`-level **Rejected**).
   - Meanwhile, [src/record.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/record.rs#L65-L66) leaves `Transaction.amount` as a signed `Amount` with the comment *"Positivity is a business rule checked by the caller."*

Put together, an agent implementing `Engine` (#5) read *"business rule"* + *"zero or negative … are Rejected"* and added `Rejection::NonPositiveAmount` inside `Engine` — which would have caused the #7 pipeline to treat it as a silent `Rejection` instead of a **Malformed** row with a stderr warning, and broke ADR 0001's rule that `Engine` always records the Transaction id first.

So the rule itself is completely sound; the only gap is making it unambiguous that the CSV adapter (or a `PositiveAmount` type on `Transaction`) enforces positivity as **Malformed** before `Engine` ever sees the `Record`.

## User [22:06]

check @[src/main.rs] i am worried that we are not using buffer reader to stream from the csv without loading the file into memory?
just investigate change nothing

## Assistant [22:07]

You are **already streaming with a buffered reader** — passing the raw `File` directly is actually the recommended usage for the `csv` crate, and the file is never loaded into memory.

Here is the full chain:

1. **[src/main.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/main.rs#L19-L27):** opens `File::open(path)` (a streaming `impl Read`, not `fs::read` / `read_to_string`) and passes it to `transaction_engine::run(...)`.
2. **[src/pipeline.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/pipeline.rs#L44-L54):** pulls one record at a time in a `while let Some(...) = records.next_record()?` loop and applies it immediately to `Engine` without collecting the input.
3. **[src/csv_adapter.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/csv_adapter.rs#L184-L213):**
   - Calls `ReaderBuilder::new().trim(Trim::All).flexible(true).from_reader(input)`.
   - Reuses a single `self.buffer: StringRecord` via `self.reader.read_record(&mut self.buffer)` so per-record allocation doesn't grow with input size either.
4. **`csv` crate (`csv 1.4.0`):** `csv::Reader` wraps the passed reader internally in `std::io::BufReader::with_capacity(...)` (`reader.rs:721, 828`). Its own docs for `ReaderBuilder::from_reader` (`reader.rs:115-116`) explicitly advise **against** adding another `BufReader`:
   > *"Note that the CSV reader is buffered automatically, so you should not wrap `rdr` in a buffered reader like `io::BufReader`."*

*(The same holds for output: `csv::Writer::from_writer` in [src/csv_adapter.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/csv_adapter.rs#L231) also buffers internally, so passing `io::stdout().lock()` directly avoids double-buffering.)*

Nothing needs to be changed.

## User [22:12]

wonder why we are not using serde, do not see any derive(deserialize or serizlize) anything.
check how much we are gain, that is a suggested crate in the exercise.

## Assistant [22:13]

Right now `serde` is declared in [Cargo.toml](file:///home/neithanmo/Documents/Projects/transaction-engine/Cargo.toml#L8) (`serde = { version = "1.0.229", features = ["derive"] }`), pulling in `serde_derive` (and `syn`/`quote`/`proc-macro2`), but **nothing in `src/` imports or uses it**. (`csv 1.4.0` already depends on `serde_core` internally, not `serde`'s derive macro.)

Here is an evaluation of what switching [src/csv_adapter.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/csv_adapter.rs) to `serde` would actually gain vs. cost.

---

### What `serde` would gain

- **Input (~15–25 lines saved):** A private `#[derive(Deserialize)] struct CsvRow<'a>` in `csv_adapter.rs` could replace [Columns](file:///home/neithanmo/Documents/Projects/transaction-engine/src/csv_adapter.rs#L96-L140)'s manual header-index lookup (`fields.get(index)`) and `u16`/`u32` parsing, using `#[serde(rename = "type")]` / `#[serde(rename = "tx")]` with `buffer.deserialize(Some(&headers))` (preserving `StringRecord` reuse).
- **Output (~0 lines saved):** A private `#[derive(Serialize)] struct AccountRow` could replace the array in [write_report](file:///home/neithanmo/Documents/Projects/transaction-engine/src/csv_adapter.rs#L227-L243) and derive header names from struct fields.

---

### Where `serde` fights our actual requirements

Because of the edge cases in this spec, `serde` does **not** eliminate most of the manual code in [src/csv_adapter.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/csv_adapter.rs):

1. **Missing header column must be Fatal, while short Dispute rows (`dispute,1,1`) must be accepted:**
   - With `serde`, accepting a 3-column `dispute,1,1` row requires `#[serde(default)]` on `amount: Option<...>`.
   - Side effect: `#[serde(default)]` also silently succeeds when the `amount` column is missing from the CSV **header**, breaking the Fatal `InputError::MissingColumn("amount")` requirement. You would still have to validate `reader.headers()` manually.
2. **Dispute actions ignore any `amount` value:**
   - Deserializing `amount` directly into `Option<Amount>` would reject a Dispute row that had non-empty/junk text in the `amount` column. You would still have to deserialize `amount: Option<&'a str>` and parse `Amount` manually for Deposits/Withdrawals anyway.
3. **Case-insensitive `type` (`Deposit`, `DEPOSIT`, `WithDrawal`):**
   - `#[serde(rename_all = "lowercase")]` only matches exact lowercase, so you still either deserialize `kind: &'a str` and run `to_ascii_lowercase()` manually or write a custom `Deserialize` impl.
4. **Empty output report (0 Accounts):**
   - `csv::Writer::serialize` only emits the header row on the *first* serialized record — if an input file produces zero Accounts, you still have to write the header manually (as [write_report](file:///home/neithanmo/Documents/Projects/transaction-engine/src/csv_adapter.rs#L232) already does).
5. **Diagnostic quality:**
   - Manual parsing gives structured [Malformed](file:///home/neithanmo/Documents/Projects/transaction-engine/src/csv_adapter.rs#L58-L73) variants (`UnknownType`, `MissingField`, `InvalidClient`, `InvalidTransactionId`, `InvalidAmount`, `NonPositiveAmount`); `serde` collapses field failures into opaque `csv::DeserializeError` strings unless you write custom deserializers.
6. **Domain types stay separate either way:**
   - [Record](file:///home/neithanmo/Documents/Projects/transaction-engine/src/record.rs#L103-L108) splits `Transaction` (has `amount`) from `DisputeAction` (no `amount`), so `serde` would only live on private CSV DTO structs inside `csv_adapter.rs`, never on the domain types.

---

### Verdict

- **Technically, you gain very little from `serde` here** (~15–20 lines on input column indexing, at the cost of extra gotchas around `#[serde(default)]` + empty-report headers + proc-macro compile time).
- **What you should not leave as-is:** keeping `serde` in [Cargo.toml](file:///home/neithanmo/Documents/Projects/transaction-engine/Cargo.toml#L8) completely unused is a red flag to a reviewer. Pick one:
  1. **Recommended — remove `serde` from `Cargo.toml`** (cleaner dependency tree, faster cold builds, and the existing [src/csv_adapter.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/src/csv_adapter.rs) is already short, zero-allocation per row, and explicit about every edge case).
  2. **Alternative — use `serde` on private `CsvRow` / `AccountRow` DTOs inside `csv_adapter.rs`** if you specifically want to showcase `serde` for the exercise, while keeping the manual header check, `amount: Option<&str>` parsing, and explicit output header.

*(No files were modified.)*

## User [22:27]

not i need to generate a nice csv and large enough with no incident or try to emulate what they expect, a sort of integration test?? that can use our lib.rs or is there already one?

y now it is iimposible to expect an llm to generate 1000 lines csv!!!
just exploring posibilities!

## Assistant [22:27]

### What already exists

You already have three integration test files in `tests/`:

- [tests/fixtures.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/tests/fixtures.rs): runs all 13 hand-crafted scenario CSVs in `tests/fixtures/` through `transaction_engine::run` in-process and compares against hand-derived `.expected.csv` files.
- [tests/pipeline.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/tests/pipeline.rs): tests fatal errors (mid-stream I/O error via a custom `Read`, missing header column) through `transaction_engine::run`.
- [tests/cli.rs](file:///home/neithanmo/Documents/Projects/transaction-engine/tests/cli.rs): spawns the compiled binary and verifies exit codes, stdout, and stderr.

What you **don't** have yet is a high-volume / realistic-scale test (thousands to millions of rows, many interleaved clients). And you're right — you shouldn't have an LLM hand-write a 1,000+ line CSV or check a giant CSV into git.

---

### Three good ways to do this

#### 1. Streaming synthetic integration test in `tests/` (best for `cargo test`)

Because `transaction_engine::run<R: Read, W: Write>` takes any `Read`, you can write a tiny deterministic `impl Read` generator in a new test (e.g., `tests/large_stream.rs`) that streams 100k–1M CSV rows on the fly without allocating the input in memory or storing any giant file in git.

To avoid a tautological test (re-implementing the engine to compute expected answers), use **cohort patterns with closed-form math** interleaved across e.g. 1,000 clients:

- **Cohort A (normal trading):** `N` cycles of deposit `10.2500` + withdraw `4.1250` → exact expected `available = N * 6.1250`, `held = 0`, `locked = false`.
- **Cohort B (dispute & resolve):** deposit `100.0`, dispute, resolve, withdraw `25.0` → `available = 75.0`, `held = 0`, `locked = false`.
- **Cohort C (fraud / chargeback / locked):** deposit `50.0`, withdraw `40.0`, dispute, chargeback, attempted withdrawal `1.0` (rejected), repayment deposit `10.0` → `available = -30.0`, `held = 0`, `total = -30.0`, `locked = true`.
- **Cohort D (open dispute):** deposit `20.0`, dispute → `available = 0`, `held = 20.0`, `total = 20.0`, `locked = false`.
- **Interleaved noise:** duplicate transaction ids, wrong-client disputes, malformed lines, whitespace/mixed-case headers — all verified to have zero net effect on balances.

**What it proves:** streaming actually works at scale, client sorting across many `u16` ids, no quadratic slowdowns, exact balances across every rule interleaved together.

#### 2. On-demand CSV generator (`examples/` binary or small Python script) for real CLI + memory profiling

A tiny deterministic generator (e.g. `cargo run --example gen_csv -- 500000 /tmp/txs.csv /tmp/expected.csv`) that writes a real file to `/tmp` (never committed) using the same cohort math. Then you can test the actual binary exactly how an evaluator runs it:

```sh
cargo build --release
/usr/bin/time -v ./target/release/transaction-engine /tmp/txs.csv > /tmp/out.csv
diff -u /tmp/expected.csv /tmp/out.csv
```

**What it proves:** real-world throughput (rows/sec) and peak RSS memory on a multi-million-row file on disk.

#### 3. Deterministic randomized invariant test (zero extra deps, or `proptest` dev-dep)

Using a tiny seeded PRNG (like a 5-line xorshift64, no new crates) to emit 50k random rows across 50 clients (valid deposits/withdrawals, random dispute/resolve/chargeback references, occasional duplicates, extreme amounts, malformed rows) and asserting **invariants** on the output CSV:

- strictly ascending `client` ids,
- every amount formatted to 4 decimal places,
- `held >= 0`,
- `total == available + held`,
- no panics or crashes regardless of row ordering or adversarial values.

**What it proves:** catches weird state-machine interactions that hand-picked scenarios miss.

---

If you want the highest value for the least complexity: **#1** (a ~100-line streaming integration test in `tests/` running 50k–100k interleaved rows with closed-form expected balances) runs automatically on every `cargo test`, and **#2** is great if you also want to time/profile the compiled binary on a 1M+ row file on disk. Let me know which direction sounds best!
