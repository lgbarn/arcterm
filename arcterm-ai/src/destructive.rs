//! Destructive command detection.

/// Warning label prepended to destructive command suggestions.
pub const WARNING_LABEL: &str = "⚠ DESTRUCTIVE";

/// Known destructive command patterns.
const DESTRUCTIVE_PATTERNS: &[&str] = &[
    "rm -rf",
    "rm -r /",
    "rm -fr",
    "rmdir /",
    "DROP TABLE",
    "DROP DATABASE",
    "TRUNCATE TABLE",
    "DELETE FROM",
    "git push --force",
    "git push -f",
    "git reset --hard",
    "git clean -fd",
    "chmod -R 777",
    "chmod 777 /",
    "chown -R",
    "dd if=",
    "mkfs",
    "fdisk",
    "parted",
    ":(){ :|:& };:",
    "> /dev/sda",
    "of=/dev/sd",
    "shutdown",
    "reboot",
    "init 0",
    "init 6",
    "kill -9 1",
    "killall",
    "pkill -9",
];

/// Check if a command string contains destructive patterns.
///
/// Returns `true` if the command matches any known destructive pattern.
/// This is a heuristic — not a security boundary. It catches common
/// dangerous operations but is not exhaustive.
pub fn is_destructive(command: &str) -> bool {
    let lower = command.to_lowercase();
    DESTRUCTIVE_PATTERNS
        .iter()
        .any(|pattern| lower.contains(&pattern.to_lowercase()))
}

/// Format a command with a destructive warning if applicable.
pub fn maybe_warn(command: &str) -> String {
    if is_destructive(command) {
        format!("{} — {}", WARNING_LABEL, command)
    } else {
        command.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rm_rf_detected() {
        assert!(is_destructive("rm -rf /tmp/data"));
        assert!(is_destructive("sudo rm -rf /"));
        assert!(is_destructive("rm -fr ./build"));
    }

    #[test]
    fn test_safe_rm_not_flagged() {
        assert!(!is_destructive("rm file.txt"));
        assert!(!is_destructive("rm -f single_file"));
    }

    #[test]
    fn test_sql_drop_detected() {
        assert!(is_destructive("DROP TABLE users;"));
        assert!(is_destructive("drop database production;"));
        assert!(is_destructive("TRUNCATE TABLE logs;"));
    }

    #[test]
    fn test_git_force_push_detected() {
        assert!(is_destructive("git push --force origin main"));
        assert!(is_destructive("git push -f"));
        assert!(is_destructive("git reset --hard HEAD~5"));
    }

    #[test]
    fn test_safe_git_not_flagged() {
        assert!(!is_destructive("git push origin main"));
        assert!(!is_destructive("git commit -m 'fix'"));
        assert!(!is_destructive("git reset --soft HEAD~1"));
    }

    #[test]
    fn test_dd_detected() {
        assert!(is_destructive("dd if=/dev/zero of=/dev/sda bs=4M"));
    }

    #[test]
    fn test_chmod_777_detected() {
        assert!(is_destructive("chmod -R 777 /var/www"));
        assert!(is_destructive("chmod 777 /"));
    }

    #[test]
    fn test_safe_chmod_not_flagged() {
        assert!(!is_destructive("chmod 644 file.txt"));
        assert!(!is_destructive("chmod +x script.sh"));
    }

    #[test]
    fn test_fork_bomb_detected() {
        assert!(is_destructive(":(){ :|:& };:"));
    }

    #[test]
    fn test_maybe_warn_adds_label() {
        let result = maybe_warn("rm -rf /tmp");
        assert!(result.starts_with(WARNING_LABEL));
    }

    #[test]
    fn test_maybe_warn_safe_unchanged() {
        let result = maybe_warn("ls -la");
        assert_eq!(result, "ls -la");
    }
}
