//! Fractional index keys for ordering columns and cards.
//!
//! A key is a string of base-62 digits (`0-9A-Za-z`, in byte order) read as a fraction
//! between 0 and 1: `"V"` is about 0.5. There is always room between two keys, so moving an
//! item only changes that item's key, which keeps git diffs (v0.4) to one line.
//! Keys never end in `0`, otherwise `"V"` and `"V0"` would be equal.

const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE: u8 = 62;

/// Why a key was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PositionError {
    #[error("invalid position key {0:?}")]
    Invalid(String),
    #[error("position keys out of order: {0:?} is not before {1:?}")]
    OutOfOrder(String, String),
}

fn digit(c: u8) -> Option<u8> {
    DIGITS.iter().position(|d| *d == c).and_then(|i| u8::try_from(i).ok())
}

fn parse(key: &str) -> Result<Vec<u8>, PositionError> {
    let digits: Option<Vec<u8>> = key.bytes().map(digit).collect();
    match digits {
        Some(d) if !d.is_empty() && d.last() != Some(&0) => Ok(d),
        _ => Err(PositionError::Invalid(key.to_owned())),
    }
}

/// `true` if `key` is a valid position key.
pub fn is_valid(key: &str) -> bool {
    parse(key).is_ok()
}

/// A key strictly between `before` and `after`. `None` means "no neighbour on that side".
pub fn between(before: Option<&str>, after: Option<&str>) -> Result<String, PositionError> {
    let a = before.map(parse).transpose()?.unwrap_or_default();
    let b = after.map(parse).transpose()?;
    if let (Some(before), Some(after)) = (before, after)
        && before >= after
    {
        return Err(PositionError::OutOfOrder(before.to_owned(), after.to_owned()));
    }
    let mid = midpoint(&a, b.as_deref());
    Ok(mid.iter().map(|d| DIGITS[usize::from(*d)] as char).collect())
}

/// Digits of a fraction strictly between `a` (missing digits = 0) and `b` (`None` = 1).
fn midpoint(a: &[u8], b: Option<&[u8]>) -> Vec<u8> {
    if let Some(b) = b {
        // Shared leading digits stay as they are.
        let shared = (0..b.len()).take_while(|&i| a.get(i).copied().unwrap_or(0) == b[i]).count();
        if shared > 0 {
            let mut out = b[..shared].to_vec();
            out.extend(midpoint(a.get(shared..).unwrap_or_default(), Some(&b[shared..])));
            return out;
        }
    }
    let da = a.first().copied().unwrap_or(0);
    let db = b.map_or(BASE, |b| b[0]);
    if db - da > 1 {
        return vec![u8::midpoint(da, db)];
    }
    // Neighbouring first digits.
    if let Some(b) = b
        && b.len() > 1
    {
        // b continues with non-zero digits, so its first digit alone is smaller than b.
        return vec![b[0]];
    }
    let mut out = vec![da];
    out.extend(midpoint(a.get(1..).unwrap_or_default(), None));
    out
}

/// Keys for `count` items in a row. Used when creating several at once.
///
/// # Panics
/// Never: appending after a valid key always works.
pub fn spread(count: usize) -> Vec<String> {
    let mut keys = Vec::with_capacity(count);
    let mut last: Option<String> = None;
    for _ in 0..count {
        let key = between(last.as_deref(), None).expect("appending always works");
        last = Some(key.clone());
        keys.push(key);
    }
    keys
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn basics() {
        assert_eq!(between(None, None).unwrap(), "V");
        let a = between(None, None).unwrap();
        let b = between(Some(&a), None).unwrap();
        let c = between(Some(&a), Some(&b)).unwrap();
        assert!(a < c && c < b, "{a} {c} {b}");
        let first = between(None, Some(&a)).unwrap();
        assert!(first < a);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(between(Some("V0"), None).is_err());
        assert!(between(Some(""), None).is_err());
        assert!(between(Some("a!"), None).is_err());
        assert!(matches!(between(Some("b"), Some("a")), Err(PositionError::OutOfOrder(..))));
        assert!(matches!(between(Some("a"), Some("a")), Err(PositionError::OutOfOrder(..))));
    }

    #[test]
    fn tight_neighbours() {
        for (a, b) in [("1", "2"), ("V", "W"), ("Vz", "W"), ("01", "02"), ("z", "zz"), ("A", "A1")] {
            let mid = between(Some(a), Some(b)).unwrap();
            assert!(a < mid.as_str() && mid.as_str() < b, "{a} < {mid} < {b}");
            assert!(is_valid(&mid));
        }
    }

    #[test]
    fn spread_is_sorted() {
        let keys = spread(50);
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    proptest! {
        /// Inserting at random spots, again and again, always gives valid keys in order.
        #[test]
        fn random_inserts_stay_ordered(spots in proptest::collection::vec(any::<prop::sample::Index>(), 1..300)) {
            let mut keys: Vec<String> = Vec::new();
            for spot in spots {
                let at = spot.index(keys.len() + 1);
                let before = at.checked_sub(1).map(|i| keys[i].as_str());
                let after = keys.get(at).map(String::as_str);
                let key = between(before, after).unwrap();
                prop_assert!(is_valid(&key));
                prop_assert!(before.is_none_or(|b| b < key.as_str()));
                prop_assert!(after.is_none_or(|a| key.as_str() < a));
                keys.insert(at, key);
            }
            prop_assert!(keys.windows(2).all(|w| w[0] < w[1]));
        }

        /// Always inserting at the front (the worst case for growth) keeps keys short enough.
        #[test]
        fn keys_stay_short(n in 1usize..500) {
            let mut first: Option<String> = None;
            for _ in 0..n {
                let key = between(None, first.as_deref()).unwrap();
                first = Some(key);
            }
            prop_assert!(first.unwrap().len() <= n / 5 + 2);
        }
    }
}
