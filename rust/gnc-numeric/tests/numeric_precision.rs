use gnc_numeric::{GncNumeric, GncNumericDenom, GncNumericRounding, GNC_DENOM_AUTO};

#[test]
fn test_sum_repeating_fractions() {
    let one_third = GncNumeric::new(1, 3);
    let mut sum = GncNumeric::zero();

    // Add 1/3 three times
    for _ in 0..3 {
        sum = GncNumeric::add(
            sum,
            one_third,
            GNC_DENOM_AUTO,
            GncNumericRounding::Never,
            GncNumericDenom::Reduce,
        );
    }

    assert_eq!(sum.num, 1);
    assert_eq!(sum.denom, 1);
}

#[test]
fn test_precision_many_splits() {
    let val = GncNumeric::new(1, 3);
    let mut sum = GncNumeric::zero();

    // Add 1/3 one thousand times
    for _ in 0..1000 {
        sum = GncNumeric::add(
            sum,
            val,
            GNC_DENOM_AUTO,
            GncNumericRounding::Never,
            GncNumericDenom::Reduce,
        );
    }

    // Result should be exactly 1000/3
    assert_eq!(sum.num, 1000);
    assert_eq!(sum.denom, 3);

    // Reducing to integer with rounding
    let rounded = GncNumeric::add(
        sum,
        GncNumeric::zero(),
        1,                         // target denom 1
        GncNumericRounding::Round, // Banker's
        GncNumericDenom::Fixed,
    );

    // 333.333... rounds to 333
    assert_eq!(rounded.num, 333);
    assert_eq!(rounded.denom, 1);
}

#[test]
fn test_mixed_denominators_exact() {
    let a = GncNumeric::new(1, 3);
    let b = GncNumeric::new(1, 6);

    // 1/3 + 1/6 = 2/6 + 1/6 = 3/6 = 1/2
    let res = GncNumeric::add(
        a,
        b,
        GNC_DENOM_AUTO,
        GncNumericRounding::Never,
        GncNumericDenom::Reduce,
    );

    assert_eq!(res.num, 1);
    assert_eq!(res.denom, 2);
}

#[test]
fn test_large_numerator_precision() {
    // Test that i128 intermediate handles large numerators correctly
    // 1,000,000,000 + 1/3
    let large = GncNumeric::new(1_000_000_000, 1);
    let frac = GncNumeric::new(1, 3);

    let res = GncNumeric::add(
        large,
        frac,
        GNC_DENOM_AUTO,
        GncNumericRounding::Never,
        GncNumericDenom::Reduce,
    );

    assert_eq!(res.num, 3_000_000_001);
    assert_eq!(res.denom, 3);
}
