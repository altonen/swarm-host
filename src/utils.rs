//! Miscellaneous utilities for `swarm-host`.

#[macro_export]
macro_rules! ensure {
    ($x:expr, $y:expr $(,)?) => {{
        if !$x {
            return Err($y);
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_works() {
        #[derive(Debug, PartialEq, Eq)]
        enum Error {
            DivideByZero,
        }

        fn func(a: usize, b: usize) -> Result<usize, Error> {
            ensure!(b != 0, Error::DivideByZero);
            Ok(a / b)
        }

        assert_eq!(func(4, 2), Ok(2));
        assert_eq!(func(4, 0), Err(Error::DivideByZero));
    }
}
