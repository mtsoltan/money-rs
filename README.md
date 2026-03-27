# money-rs

A CURD application to track and manage how I gain, use, and spend money across multiple currencies,
while maintaining quasi-objective sense of value regardless of how currencies change in worth over time.

Currently a WIP with horrible version history. Version control will only be taken seriously after version `1.0.0`.

Written in Rust as a chance to not get rusty on Rust (pun intended).

Current progress:
- Schema - Done
- CURD - Done
- Backend advanced operations - Done
- Functionality consolidation and backend integration tests - Halfway done
- Front end - Not started
- Proper readme and documentation - Not started
- Define codebase standards and adhere to them - Not started

License: Modified MIT (two extra clause regarding informing the author in case of commercial use, and not using software to train LLMs)

## Important Commands

Setup
```bash
cargo update
```

Migration
```bash
sudo -u postgres /home/USERNAME/.cargo/bin/diesel database setup

diesel migration redo
diesel print-schema
diesel database reset

# Extra
diesel migration revert
diesel migration down
```

Database
```bash
sudo systemctl start postgresql@16-main.service
psql "postgresql://money:money@localhost:5432/money"
psql "postgresql://money:money@localhost:5432/money_test"

# Extra
sudo systemctl restart postgresql@16-main.service
sudo -u postgres psql
sudo -u postgres psql -W -l
sudo -u postgres fish
```

Format and build
```bash
cargo fmt \*.rs

# Extra
cargo build
cargo fmt
cargo check

```

Expand
```bash
RUSTFLAGS='--cfg test' cargo +nightly expand --features create_user > expand.rs
cargo +nightly expand --features create_user Entry
cargo +nightly expand --features create_user model.rs
```

Backend and its tests
```bash
cargo run -p money-rs --features create_user
cargo test -- --nocapture

# Extra
RUST_BACKTRACE=1 cargo test -- --nocapture
RUST_BACKTRACE=1 cargo +nightly test --features create_user -- --nocapture
RUSTC_WRAPPER=sccache cargo build --release
```

Frontend
```bash
cd frontend
trunk watch

# Extra
trunk serve
trunk build --release
```
