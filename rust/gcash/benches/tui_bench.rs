use criterion::{criterion_group, criterion_main, Criterion};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use gcash::tui::{App, Event};
use gnucash_engine::domain::{
    Account, AccountType, CommodityId, DraftTransaction, Ledger, Split, Transaction,
};
use num_rational::Rational64;
use std::collections::HashMap;
use std::hint::black_box;

fn create_large_ledger(num_accounts: usize, num_transactions: usize) -> Ledger {
    let commodity = CommodityId::new("USD");
    let mut accounts = Vec::with_capacity(num_accounts);
    for i in 0..num_accounts {
        accounts.push(Account::new(
            format!("Account {}", i),
            AccountType::Expense,
            commodity.clone(),
        ));
    }

    let mut transactions = Vec::with_capacity(num_transactions);
    for i in 0..num_transactions {
        let acc1_idx = i % num_accounts;
        let acc2_idx = (i + 1) % num_accounts;

        let mut draft =
            DraftTransaction::new(commodity.clone()).with_description(format!("Transaction {}", i));

        draft = draft.add_split(Split::new(accounts[acc1_idx].id, Rational64::new(100, 1)));
        draft = draft.add_split(Split::new(accounts[acc2_idx].id, Rational64::new(-100, 1)));

        transactions.push(draft.validate().unwrap());
    }

    Ledger::new(accounts, transactions)
}

fn bench_tui_responsiveness(c: &mut Criterion) {
    let num_accounts = 200;
    let num_transactions = 1_000_000;

    println!(
        "Generating ledger with {} accounts and {} transactions...",
        num_accounts, num_transactions
    );
    let ledger = create_large_ledger(num_accounts, num_transactions);
    let key_map = HashMap::new(); // Empty for bench

    c.bench_function("App::new with 1M transactions", |b| {
        b.iter(|| {
            let app = App::new(ledger.clone(), key_map.clone());
            black_box(app);
        })
    });

    let mut app = App::new(ledger, key_map);

    // Bench tab switch (MoveRight)
    let move_right_event = Event::Key(KeyEvent {
        code: KeyCode::Right,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });

    c.bench_function("App::update (tab switch) 1M transactions", |b| {
        b.iter(|| {
            app.update(black_box(move_right_event));
            black_box(&app.tab_index);
        })
    });

    // Bench row calculation for a specific account (this is what happens during render)
    c.bench_function(
        "TUI row filtering with query 1M tx across 200 accounts",
        |b| {
            b.iter(|| {
                let active_id = app.active_accounts[app.tab_index % app.active_accounts.len()];
                let query = "999";
                let rows: Vec<&Transaction> = app
                    .ledger
                    .transactions
                    .iter()
                    .filter(|tx| tx.splits().iter().any(|s| s.account_id == active_id))
                    .filter(|tx| tx.description().to_lowercase().contains(query))
                    .collect();
                black_box(rows);
            })
        },
    );

    // MASSIVE SINGLE ACCOUNT CASE
    let mut massive_accounts = Vec::new();
    let commodity = CommodityId::new("USD");
    let main_acc = Account::new("Main", AccountType::Bank, commodity.clone());
    let other_acc = Account::new("Other", AccountType::Expense, commodity.clone());
    massive_accounts.push(main_acc.clone());
    massive_accounts.push(other_acc.clone());

    let mut massive_txs = Vec::with_capacity(num_transactions);
    for i in 0..num_transactions {
        let mut draft =
            DraftTransaction::new(commodity.clone()).with_description(format!("Tx {}", i));
        draft = draft.add_split(Split::new(main_acc.id, Rational64::new(100, 1)));
        draft = draft.add_split(Split::new(other_acc.id, Rational64::new(-100, 1)));
        massive_txs.push(draft.validate().unwrap());
    }
    let massive_ledger = Ledger::new(massive_accounts, massive_txs);
    let massive_app = App::new(massive_ledger, HashMap::new());

    c.bench_function("TUI row filtering 1M tx in SINGLE account", |b| {
        b.iter(|| {
            let active_id = massive_app.active_accounts[0];
            let query = "999";
            let rows: Vec<&Transaction> = massive_app
                .ledger
                .transactions
                .iter()
                .filter(|tx| tx.splits().iter().any(|s| s.account_id == active_id))
                .filter(|tx| tx.description().to_lowercase().contains(query))
                .collect();
            black_box(rows);
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench_tui_responsiveness
}
criterion_main!(benches);
