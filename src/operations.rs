use num::{bigint::BigInt, rational::BigRational, Signed, Zero};

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

#[cfg(test)]
mod operation_tests {
    use crate::{
        operations::make_decimal_string,
        syntax_tree::SyntaxTree,
        token::{ParsedInput, Tokenizer},
    };

    fn evaluate_to_string(
        input: &str,
        parse_radix: u8,
        result_radix: u8,
        precision: u8,
        commas: bool,
        upper: bool,
    ) -> String {
        let tokenizer = Tokenizer::new();
        let tokens = match tokenizer.tokenize(input, parse_radix).unwrap() {
            ParsedInput::Tokens(t) => t,
            ParsedInput::Command((_, _)) => panic!(),
        };
        let st = SyntaxTree::new(tokens.into()).unwrap();
        let result = st.execute(None, None, None).unwrap();
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
}
