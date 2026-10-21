//! How a size is said, the same in the workspace and in the CLI.

/// `812 KB`, `1.4 GB`: the unit that keeps the number short, in multiples
/// of 1024.
pub fn bytes_to_say(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::bytes_to_say;

    #[test]
    fn a_size_is_said_in_the_unit_that_keeps_it_short() {
        assert_eq!(bytes_to_say(512), "512 B");
        assert_eq!(bytes_to_say(812 * 1024), "812 KB");
        assert_eq!(bytes_to_say(1536 * 1024 * 1024), "1.5 GB");
    }
}
