use criterion::{criterion_group, criterion_main, Criterion};
use gnucash_engine::domain::{AccountId, CommodityId, DraftTransaction, Split, Transaction};
use num_rational::Rational64;
use num_traits::Signed;
use std::hint::black_box;

fn create_million_transactions(c: &mut Criterion) {
    let commodity = CommodityId::new("USD");
    let account1 = AccountId::new();
    let account2 = AccountId::new();

    c.bench_function("create 100k transactions", |b| {
        b.iter(|| {
            let mut txs = Vec::with_capacity(100_000);
            for i in 0..100_000 {
                let mut draft = DraftTransaction::new(commodity.clone())
                    .with_description(format!("Transaction {}", i));

                draft = draft.add_split(Split::new(account1, Rational64::new(100, 1)));
                draft = draft.add_split(Split::new(account2, Rational64::new(-100, 1)));

                let tx = draft.validate().unwrap();
                txs.push(tx);
            }
            black_box(txs);
        })
    });
}

fn process_million_transactions(c: &mut Criterion) {
    let commodity = CommodityId::new("USD");
    let account1 = AccountId::new();
    let account2 = AccountId::new();

    let mut transactions = Vec::with_capacity(1_000_000);
    for i in 0..1_000_000 {
        let mut draft =
            DraftTransaction::new(commodity.clone()).with_description(format!("Transaction {}", i));

        draft = draft.add_split(Split::new(account1, Rational64::new(i as i64, 1)));
        draft = draft.add_split(Split::new(account2, Rational64::new(-(i as i64), 1)));

        let tx = draft.validate().unwrap();
        transactions.push(tx);
    }

    c.bench_function("sum million transactions volume", |b| {
        b.iter(|| {
            let total_volume: Rational64 = transactions
                .iter()
                .map(|tx| {
                    tx.splits()
                        .iter()
                        .map(|s| s.amount.abs())
                        .sum::<Rational64>()
                })
                .sum();
            black_box(total_volume);
        })
    });

    c.bench_function("filter million transactions by description", |b| {
        b.iter(|| {
            let filtered: Vec<&Transaction> = transactions
                .iter()
                .filter(|tx| tx.description().contains("9999"))
                .collect();
            black_box(filtered);
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = create_million_transactions, process_million_transactions
}
criterion_main!(benches);
