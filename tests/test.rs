//! Integration tests live under `tests/`. Each `.rs` file here is built as its own crate that
//! depends on your library (`nlreminder`). Run everything with:
//!
//! ```text
//! cargo test
//! ```
//!
//! Run only this file’s tests:
//!
//! ```text
//! cargo test --test test
//! ```

/// A plain function can be tested directly. Helpers used only from tests are often kept in the
/// same test module or in `tests/common.rs` with `mod common;`.
fn double(x: i32) -> i32 {
    x * 2
}

#[test]
fn basic_assertions() {
    assert!(true);
    assert!(double(3) == 6, "optional message when the condition fails");
}

#[test]
fn equality_macros() {
    assert_eq!(double(0), 0);
    assert_ne!(double(1), 3);
}

#[test]
fn comparing_floats() {
    let left: f64 = 0.1 + 0.2;
    let right: f64 = 0.3;
    assert!((left - right).abs() < f64::EPSILON);
}

#[test]
#[should_panic(expected = "divide by zero")]
fn should_panic_example() {
    fn divide(a: i32, b: i32) -> i32 {
        if b == 0 {
            panic!("divide by zero");
        }
        a / b
    }

    let _ = divide(1, 0);
}

#[test]
fn result_and_question_mark_pattern() {
    fn parse_positive(s: &str) -> Result<u32, &'static str> {
        let n: u32 = s.parse().map_err(|_| "not a number")?;
        if n == 0 {
            return Err("not positive");
        }
        Ok(n)
    }

    assert_eq!(parse_positive("5").unwrap(), 5);
    assert!(parse_positive("0").is_err());
}

#[tokio::test]
async fn async_test_example() {
    async fn increment(x: i32) -> i32 {
        x + 1
    }

    assert_eq!(increment(40).await, 41);
}

// To call items from your crate, they must be `pub` in `src/lib.rs`. Example:
//
//     use nlreminder::some_public_fn;
//     assert_eq!(some_public_fn(), 42);
