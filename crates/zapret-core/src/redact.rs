//! Masking for the full report: IP addresses and the user's own domains are
//! replaced before the text leaves the computer (owner's decision,
//! 17.11.2026: the report may carry a masked engine log tail).

pub const MASK: &str = "***";

/// `text` with every IPv4 and IPv6 address and every domain from `domains`
/// (case-insensitive) replaced by [`MASK`].
pub fn redact(text: &str, domains: &[String]) -> String {
    let mut out = mask_addresses(text);
    for domain in domains {
        let domain = domain.trim();
        if domain.len() >= 3 {
            out = replace_ignore_case(&out, domain, MASK);
        }
    }
    out
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn mask_addresses(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let starts_run = c.is_ascii_hexdigit() || c == ':';
        let after_word = i > 0 && (is_word(chars[i - 1]) || chars[i - 1] == '.');
        if !starts_run || after_word {
            out.push(c);
            i += 1;
            continue;
        }
        let mut end = i;
        while end < chars.len()
            && (chars[end].is_ascii_hexdigit() || chars[end] == ':' || chars[end] == '.')
        {
            end += 1;
        }
        // A trailing dot or colon ends a sentence or a label, not an address.
        while end > i && matches!(chars[end - 1], '.' | ':') {
            end -= 1;
        }
        if end == i {
            out.push(c);
            i += 1;
            continue;
        }
        let run: String = chars[i..end].iter().collect();
        let before_word = end < chars.len() && is_word(chars[end]);
        if before_word {
            out.push_str(&run);
        } else if is_ipv4(&run) || is_ipv6(&run) {
            out.push_str(MASK);
        } else if let Some((host, port)) = run.split_once(':')
            && is_ipv4(host)
            && port.chars().all(|c| c.is_ascii_digit())
        {
            out.push_str(MASK);
            out.push(':');
            out.push_str(port);
        } else {
            out.push_str(&run);
        }
        i = end;
    }
    out
}

fn is_ipv4(run: &str) -> bool {
    let parts: Vec<&str> = run.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()))
        && parts
            .iter()
            .all(|p| p.parse::<u16>().is_ok_and(|n| n <= 255))
}

fn is_ipv6(run: &str) -> bool {
    let colons = run.matches(':').count();
    if colons < 2 || run.contains(":::") {
        return false;
    }
    // An embedded IPv4 tail (::ffff:1.2.3.4) counts as the last group.
    let (head, v4_groups) = match run.rsplit_once(':') {
        Some((head, tail)) if tail.contains('.') => {
            if !is_ipv4(tail) {
                return false;
            }
            (head, 1)
        }
        _ => (run, 0),
    };
    let groups: Vec<&str> = head.split(':').collect();
    let filled = groups.iter().filter(|g| !g.is_empty()).count() + v4_groups;
    let well_formed = groups
        .iter()
        .all(|g| g.len() <= 4 && g.chars().all(|c| c.is_ascii_hexdigit()));
    // Clock times (12:34:56) have no "::" and fewer than 7 colons.
    well_formed && filled >= 2 && (run.contains("::") || colons >= 7)
}

fn replace_ignore_case(text: &str, needle: &str, with: &str) -> String {
    let lower = text.to_lowercase();
    let needle = needle.to_lowercase();
    // Lowercasing may change byte lengths outside ASCII; then match exactly.
    if lower.len() != text.len() {
        return text.replace(&needle, with);
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (at, _) in lower.match_indices(&needle) {
        out.push_str(&text[last..at]);
        out.push_str(with);
        last = at + needle.len();
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_masked() {
        let text = "connect 149.154.167.220:443 failed; dns 8.8.8.8, v6 2a00:1450:4010::8a and \
                    [fe80::1%12] and ::ffff:10.0.0.1 and 2001:db8:0:0:0:0:0:1.";
        assert_eq!(
            redact(text, &[]),
            "connect ***:443 failed; dns ***, v6 *** and [***%12] and *** and ***."
        );
    }

    #[test]
    fn versions_times_and_code_stay() {
        let text = "Zapret 0.2.4 at 12:34:56, bundle 2026.11.14.2200, zapretd::service, \
                    winws 1.10.4, sha 7fe0fdbe, port 1443, 999.1.1.1, a1.2.3.4";
        assert_eq!(redact(text, &[]), text);
    }

    #[test]
    fn user_domains_are_masked_ignoring_case() {
        let domains = vec!["example.org".to_string(), "  ".to_string()];
        assert_eq!(
            redact("open Example.ORG and www.example.org", &domains),
            "open *** and www.***"
        );
    }
}
