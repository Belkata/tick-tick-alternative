//! Fractional sort keys for manual order (plan/04 `sort_key`).
//!
//! Keys are base-36 strings (`0-9a-z`) that compare as fractions, so a new key can
//! always go between two others and moving one item is a single op. SQLite's
//! default text comparison is bytewise, which matches. A key never ends in `0`,
//! so there is always room before it. The empty key means "never arranged".

const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const BASE: usize = DIGITS.len();

fn digit(c: u8) -> usize {
    DIGITS.iter().position(|&d| d == c).unwrap_or(0)
}

/// A key strictly between `a` and `b`. `None` for `a` is the start, for `b` the end.
/// Expects `a < b` and keys made by this module.
pub(crate) fn key_between(a: Option<&str>, b: Option<&str>) -> String {
    midpoint(a.unwrap_or("").as_bytes(), b.map(str::as_bytes))
}

fn midpoint(a: &[u8], b: Option<&[u8]>) -> String {
    if let Some(b) = b {
        // Keep the common prefix (`a` reads as padded with zeros).
        let n = (0..b.len())
            .take_while(|&i| a.get(i).copied().unwrap_or(b'0') == b[i])
            .count();
        if n > 0 {
            let rest = midpoint(a.get(n..).unwrap_or(&[]), Some(&b[n..]));
            return format!("{}{rest}", String::from_utf8_lossy(&b[..n]));
        }
    }
    let da = a.first().map_or(0, |&c| digit(c));
    let db = b.map_or(BASE, |b| b.first().map_or(BASE, |&c| digit(c)));
    if db.saturating_sub(da) > 1 {
        return (DIGITS[(da + db).div_ceil(2)] as char).to_string();
    }
    // Adjacent digits: a longer `b` already has room right after its first digit.
    if let Some(b) = b.filter(|b| b.len() > 1) {
        return (b[0] as char).to_string();
    }
    format!(
        "{}{}",
        DIGITS[da] as char,
        midpoint(a.get(1..).unwrap_or(&[]), None)
    )
}

/// `n` increasing keys, evenly spread, for renumbering a whole set of siblings.
pub(crate) fn spread_keys(n: usize) -> Vec<String> {
    let mut width = 1u32;
    while BASE.pow(width) < (n + 1) * BASE {
        width += 1;
    }
    let step = BASE.pow(width) / (n + 1);
    (1..=n)
        .map(|i| {
            let mut v = i * step;
            let mut s = vec![b'0'; width as usize];
            for slot in s.iter_mut().rev() {
                *slot = DIGITS[v % BASE];
                v /= BASE;
            }
            // Trailing zeros don't change the order among equal-width keys.
            while s.len() > 1 && s.last() == Some(&b'0') {
                s.pop();
            }
            String::from_utf8(s).expect("ascii")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn simple_cases() {
        assert_eq!(key_between(None, None), "i");
        assert!(key_between(None, Some("i")).as_str() < "i");
        assert!(key_between(Some("i"), None).as_str() > "i");
        let k = key_between(Some("a"), Some("b"));
        assert!("a" < k.as_str() && k.as_str() < "b", "{k}");
        let k = key_between(Some("a"), Some("a1"));
        assert!("a" < k.as_str() && k.as_str() < "a1", "{k}");
        let keys = spread_keys(50);
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        assert!(keys.iter().all(|k| !k.ends_with('0') && !k.is_empty()));
    }

    proptest! {
        /// Inserting at random places keeps every key unique, ordered and zero-free at the end.
        #[test]
        fn random_inserts_stay_ordered(spots in prop::collection::vec(0usize..1000, 1..200)) {
            let mut keys: Vec<String> = Vec::new();
            for spot in spots {
                let i = spot % (keys.len() + 1);
                let a = i.checked_sub(1).map(|j| keys[j].as_str());
                let b = keys.get(i).map(String::as_str);
                let k = key_between(a, b);
                prop_assert!(a.is_none_or(|a| a < k.as_str()), "{a:?} < {k}");
                prop_assert!(b.is_none_or(|b| k.as_str() < b), "{k} < {b:?}");
                prop_assert!(!k.ends_with('0'));
                keys.insert(i, k);
            }
        }
    }
}
