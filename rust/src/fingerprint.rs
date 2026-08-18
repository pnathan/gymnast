/// FNV-1a 64 over the UTF-8 bytes of `text`, formatted like the Lamedh
/// implementation: "fnv1a64:" + the hash printed as a SIGNED i64.
pub fn fingerprint_string(text: &str) -> String {
    let mut hash: u64 = 0xCBF29CE484222325;
    for b in text.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("fnv1a64:{}", hash as i64)
}

/// Fingerprint of a value: hash of its canonical print (no newline).
pub fn fingerprint(value: &crate::sexpr::Sexpr) -> String {
    fingerprint_string(&value.print())
}
