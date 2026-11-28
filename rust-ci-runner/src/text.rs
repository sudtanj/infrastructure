//! Text that reaches a status description or the Actions log (FR-051).

/// Characters outside `[A-Za-z0-9 ,_.:/-]` become `_`, a leading `::` (a workflow command
/// marker) gets a `_` prefix, and the result is cut to `max` characters.
pub fn sanitize(s: &str, max: usize) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || " ,_.:/-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.starts_with("::") {
        out.insert(0, '_');
    }
    out.chars().take(max).collect()
}
