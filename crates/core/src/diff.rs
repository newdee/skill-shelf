use similar::{ChangeTag, TextDiff};

/// Produce a line-oriented text diff for two byte blobs.
///
/// Returns `None` when either side is not valid UTF-8 (i.e. a binary file):
/// callers should treat that as "changed but not renderable as text".
pub fn text_diff(old: &[u8], new: &[u8]) -> Option<String> {
    let old = std::str::from_utf8(old).ok()?;
    let new = std::str::from_utf8(new).ok()?;

    let diff = TextDiff::from_lines(old, new);
    let mut out = String::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        out.push_str(sign);
        out.push_str(change.value());
        if !change.value().ends_with('\n') {
            out.push('\n');
        }
    }
    Some(out)
}
