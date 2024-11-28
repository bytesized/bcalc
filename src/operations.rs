use crate::error::MathExecutionError::{self, ImaginaryResult};

use num::{
    bigint::BigInt, pow::Pow, rational::BigRational, traits::Inv, BigUint, Integer, Signed, Zero,
};

/// `BigRational` only seems to support fractional string conversion, but we want to support decimal
/// output as well.
/// We want to display trailing zeros, but in a way such that they are actually significant. We are
/// only going to display them in order to indicate that we are rounding and the number isn't
/// precise. For example:
///   `make_decimal_string(0.01, 10, 5, false) == "0.01"`
///   `make_decimal_string(0.010001, 10, 5, false) == "0.01000"`
pub fn make_decimal_string(
    value: &BigRational,
    radix: u8,
    precision: u8,
    commas: bool,
    upper: bool,
) -> String {
    // We need to split off the negative sign now rather than retaining it in the integer part of
    // the value. Otherwise if the integer portion of the number is `0`, the sign won't get
    // displayed properly. Plus, as a side benefit, we don't have to think about negative modulus.
    let sign_str = if value < &BigRational::zero() {
        "-"
    } else {
        ""
    };
    let radix_power = BigInt::from(radix).pow(precision as u32);
    let multiplied_value = (value * &radix_power).abs();
    let value_precisely_represented = multiplied_value.is_integer();
    let rounded = multiplied_value.round().to_integer();
    let int_value = &rounded / &radix_power;
    let fractional_string = if precision == 0 {
        String::new()
    } else {
        let fractional_value = &rounded % &radix_power;
        let mut fractional_string = format!(
            "{:0>width$}",
            fractional_value.to_str_radix(radix as u32),
            width = precision as usize
        );
        if upper {
            fractional_string.make_ascii_uppercase();
        }
        if value_precisely_represented {
            fractional_string.trim_end_matches('0').to_string()
        } else {
            fractional_string
        }
    };
    let mut int_string = int_value.to_str_radix(radix as u32);
    if upper {
        int_string.make_ascii_uppercase();
    }
    let int_string_commas: String = if commas {
        int_string
            .chars()
            .collect::<Vec<char>>()
            .rchunks(3)
            .rev()
            .map(|s| String::from_iter(s.into_iter()))
            .collect::<Vec<String>>()
            .join(",")
    } else {
        int_string
    };

    if fractional_string.is_empty() {
        format!("{}{}", sign_str, int_string_commas)
    } else {
        format!("{}{}.{}", sign_str, int_string_commas, fractional_string)
    }
}

pub fn exponentiate(
    mut base: BigRational,
    exponent: BigRational,
    precision: u8,
    radix: u8,
) -> Result<BigRational, MathExecutionError> {
    // Step 1: If necessary, convert `b^-(n/d)` to `(1/b)^(n/d)`.
    if exponent.is_negative() {
        base = base.inv();
    }

    let (exp_num, exp_denom) = match exponent.into_raw() {
        (num, denom) => (
            num.abs().to_biguint().unwrap(),
            denom.abs().to_biguint().unwrap(),
        ),
    };

    // Step 2: Convert `b^(n/d)` to `(b^n)^(1/d)` and compute `r = b^n` so we are left with
    // `r^(1/d)`.
    let radicand = base.pow(exp_num);

    // Step 3: Newton's Method
    // Given `r` and `d`, we want to compute `r^(1/d)`. We will call the result `x`.
    // So `r^(1/d) = x`, or `r = x^d`.
    // Newton's method concerns finding when a function reaches 0. So we will make our function
    // `f(x) = x^d - r`.
    // Newton's method can be summarized as iterations of `x_n+1 = x_n - f(x_n)/f'(x_n)`.
    // (Using `x` for `x_n` below, for brevity)
    // `x_n+1 = x - (x^d - r)/(d * x^(d - 1))`
    // `x_n+1 = x + (r - x^d)/(d * x^(d - 1))`
    // `x_n+1 = (r - x^d + (x * d * x^(d - 1)))/(d * x^(d - 1))`
    // `x_n+1 = (r - x^d + d*x^d)/(d * x^(d - 1))
    // `x_n+1 = (r + (d - 1)*x^d)/(d * x^(d - 1))

    // Step 3.1: Rename, convert, and pre-calculate a few things.
    let degree = exp_denom;
    let one = BigUint::from(1u8);
    let one_signed = BigInt::from(1);
    let degree_ratio: BigRational = BigRational::from(BigInt::from(degree.clone()));
    let degree_dec: BigUint = &degree - &one;
    let degree_dec_ratio: BigRational = BigRational::from(BigInt::from(degree_dec.clone()));
    // We are actually going to add one additional digit of precision. This prevents a rounding
    // error from making our last guaranteed digit wrong.
    let precision = BigUint::from(precision + 1);
    let radix = BigInt::from(radix);
    // The largest amount we are okay with being wrong by.
    let max_error = BigRational::new(one_signed.clone(), radix.pow(precision).into());
    let f_magnitude = |x: &BigInt| -> BigRational {
        (BigRational::from(x.clone()).pow(&degree) - &radicand).abs()
    };
    let next_x = |x: BigRational| -> BigRational {
        (&radicand + &degree_dec_ratio * x.clone().pow(&degree))
            / (&degree_ratio * x.pow(&degree_dec))
    };

    // We are already done.
    if degree == one {
        return Ok(radicand);
    }

    // Step 3.2: Input validation. This function currently cannot output complex numbers.
    if radicand.is_negative() && degree.is_even() {
        return Err(ImaginaryResult);
    }

    // Step 3.3: Use a binary search to find a good starting point for Newton's method. Otherwise
    // it takes forever.
    let mut x = {
        let (mut lower_bound, mut upper_bound) = if radicand.is_negative() {
            (radicand.to_integer(), BigInt::from(0))
        } else {
            (BigInt::from(0), radicand.to_integer())
        };

        // We are going to unroll a bit to make the loop less confusing.
        let mut guess: BigInt = (&upper_bound - &lower_bound) / 2 + &lower_bound;
        let mut error = f_magnitude(&guess);
        let (mut last_guess_was_lower, mut last_error) = {
            let next_guess = &guess + 1;
            let next_error = f_magnitude(&next_guess);
            if next_error < error {
                // We want to head towards the upper bound
                lower_bound = next_guess;
                (true, next_error)
            } else {
                // We want to head towards the lower bound
                upper_bound = guess;
                (false, error)
            }
        };

        let mut span = &upper_bound - &lower_bound;
        while span > one_signed {
            guess = span / 2 + &lower_bound;
            error = f_magnitude(&guess);
            if last_guess_was_lower == (error < last_error) {
                // Error is decreasing in the positive direction, we want to head towards the
                // upper bound.
                last_guess_was_lower = true;
                lower_bound = guess;
            } else {
                // Error is decreasing in the negative direction. We want to head towards the lower
                // bound.
                last_guess_was_lower = false;
                upper_bound = guess;
            }

            last_error = error;
            span = &upper_bound - &lower_bound;
        }

        guess = if span.is_zero() || f_magnitude(&upper_bound) < f_magnitude(&lower_bound) {
            upper_bound
        } else {
            lower_bound
        };

        let guess_error = f_magnitude(&guess);
        let guess_ratio = BigRational::from(guess);
        // Return early if it's an exact integer.
        if guess_error.is_zero() {
            return Ok(guess_ratio);
        }

        guess_ratio
    };

    // Step 3.4: Newton's method
    loop {
        let prev_x = x.clone();
        x = next_x(x);
        let error = (&x - prev_x).abs();
        if error <= max_error {
            break;
        }
    }

    Ok(x)
}

#[cfg(test)]
mod operation_tests {
    use crate::{
        operations::make_decimal_string,
        syntax_tree::SyntaxTree,
        token::{ParsedInput, Tokenizer},
        Args,
    };

    fn evaluate_to_string(
        input: &str,
        parse_radix: u8,
        result_radix: u8,
        precision: u8,
        commas: bool,
        upper: bool,
    ) -> String {
        let args = Args {
            radix: parse_radix,
            input: None,
            alternate_screen: false,
            no_db: true,
            convert_to_radix: Some(result_radix),
            precision,
            extra_precision: 0,
            fractional: false,
            commas,
            upper,
        };
        let tokenizer = Tokenizer::new();
        let tokens = match tokenizer.tokenize(input, parse_radix).unwrap() {
            ParsedInput::Tokens(t) => t,
            ParsedInput::Command((_, _)) => panic!(),
        };
        let st = SyntaxTree::new(tokens.into()).unwrap();
        let result = st.execute(None, None, None, &args).unwrap();
        make_decimal_string(&result, result_radix, precision, commas, upper)
    }

    #[test]
    fn decimal_value() {
        let result = evaluate_to_string("1234567890", 10, 10, 5, false, false);
        assert_eq!(result, "1234567890".to_string());
    }

    #[test]
    fn negative_decimal_value() {
        let result = evaluate_to_string("-1234567890", 10, 10, 5, false, false);
        assert_eq!(result, "-1234567890".to_string());
    }

    #[test]
    fn hex_value() {
        let result = evaluate_to_string("-1234567890ABCDEF", 16, 16, 5, false, false);
        assert_eq!(result, "-1234567890abcdef".to_string());
    }

    #[test]
    fn hex_value_upper() {
        let result = evaluate_to_string("-1234567890ABCDEF.12A", 16, 16, 5, false, true);
        assert_eq!(result, "-1234567890ABCDEF.12A".to_string());
    }

    #[test]
    fn precise_small_decimal() {
        let result = evaluate_to_string("0.01", 10, 10, 5, false, false);
        assert_eq!(result, "0.01".to_string());
    }

    #[test]
    fn rounded_small_decimal() {
        let result = evaluate_to_string("0.010001", 10, 10, 5, false, false);
        assert_eq!(result, "0.01000".to_string());
    }

    #[test]
    fn unrounded_small_decimal() {
        let result = evaluate_to_string("0.010001", 10, 10, 6, false, false);
        assert_eq!(result, "0.010001".to_string());
    }

    #[test]
    fn precise_large_decimal() {
        let result = evaluate_to_string("-12345.01", 10, 10, 5, false, false);
        assert_eq!(result, "-12345.01".to_string());
    }

    #[test]
    fn rounded_large_decimal() {
        let result = evaluate_to_string("-12345.010001", 10, 10, 5, false, false);
        assert_eq!(result, "-12345.01000".to_string());
    }

    #[test]
    fn unrounded_large_decimal() {
        let result = evaluate_to_string("-12345.010001", 10, 10, 6, false, false);
        assert_eq!(result, "-12345.010001".to_string());
    }

    #[test]
    fn zero() {
        let result = evaluate_to_string("0", 10, 10, 5, false, false);
        assert_eq!(result, "0".to_string());
    }

    #[test]
    fn commas_1() {
        let result = evaluate_to_string("123456789", 10, 10, 5, true, false);
        assert_eq!(result, "123,456,789".to_string());
    }

    #[test]
    fn commas_2() {
        let result = evaluate_to_string("1234567890", 10, 10, 5, true, false);
        assert_eq!(result, "1,234,567,890".to_string());
    }

    #[test]
    fn commas_3() {
        let result = evaluate_to_string("12345678901", 10, 10, 5, true, false);
        assert_eq!(result, "12,345,678,901".to_string());
    }

    #[test]
    fn round_down() {
        let result = evaluate_to_string("0.0000049", 10, 10, 5, false, false);
        assert_eq!(result, "0.00000".to_string());
    }

    #[test]
    fn round_up() {
        let result = evaluate_to_string("0.000005", 10, 10, 5, false, false);
        assert_eq!(result, "0.00001".to_string());
    }

    #[test]
    fn no_precision_zero() {
        let result = evaluate_to_string("0.1", 10, 10, 0, false, false);
        assert_eq!(result, "0".to_string());
    }

    #[test]
    fn no_precision_positive() {
        let result = evaluate_to_string("1.1", 10, 10, 0, false, false);
        assert_eq!(result, "1".to_string());
    }

    #[test]
    fn no_precision_negative() {
        let result = evaluate_to_string("-1.1", 10, 10, 0, false, false);
        assert_eq!(result, "-1".to_string());
    }

    #[test]
    fn root_integer_result_1() {
        let result = evaluate_to_string("100^(1/2)", 10, 10, 10, false, false);
        assert_eq!(result, "10".to_string());
    }

    #[test]
    fn root_integer_result_2() {
        let result = evaluate_to_string("59049^(1/10)", 10, 10, 10, false, false);
        assert_eq!(result, "3".to_string());
    }

    #[test]
    fn exponentiate_integer_result_1() {
        let result = evaluate_to_string("9^10", 10, 10, 10, false, false);
        assert_eq!(result, "3486784401".to_string());
    }

    #[test]
    fn exponentiate_integer_result_2() {
        let result = evaluate_to_string("-3^9", 10, 10, 10, false, false);
        assert_eq!(result, "-19683".to_string());
    }

    #[test]
    fn exponentiate_integer_result_3() {
        let result = evaluate_to_string("-3^10", 10, 10, 10, false, false);
        assert_eq!(result, "59049".to_string());
    }

    #[test]
    fn exponentiate_fractional_result_1() {
        let result = evaluate_to_string("2^(1/2)", 10, 10, 10, false, false);
        assert_eq!(result, "1.4142135624".to_string());
    }

    #[test]
    fn exponentiate_fractional_result_2() {
        let result = evaluate_to_string("-2^(1/3)", 10, 10, 10, false, false);
        assert_eq!(result, "-1.2599210499".to_string());
    }

    #[test]
    fn exponentiate_fractional_result_3() {
        let result = evaluate_to_string("-100^(7/3)", 10, 10, 10, false, false);
        assert_eq!(result, "-46415.8883361278".to_string());
    }

    #[test]
    fn exponentiate_by_zero() {
        let result = evaluate_to_string("-100^0", 10, 10, 10, false, false);
        assert_eq!(result, "1".to_string());
    }

    #[test]
    fn exponentiate_zero() {
        let result = evaluate_to_string("0^2", 10, 10, 10, false, false);
        assert_eq!(result, "0".to_string());
    }

    #[test]
    fn exponentiate_zero_by_zero() {
        let result = evaluate_to_string("0^0", 10, 10, 10, false, false);
        assert_eq!(result, "1".to_string());
    }

    #[test]
    fn exponentiate_by_negative() {
        let result = evaluate_to_string("10^-2", 10, 10, 10, false, false);
        assert_eq!(result, "0.01".to_string());
    }

    #[test]
    fn exponentiate_one() {
        let result = evaluate_to_string("1^(999/998)", 10, 10, 10, false, false);
        assert_eq!(result, "1".to_string());
    }
}
