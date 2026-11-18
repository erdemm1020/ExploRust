//! Quoting for the shell (a path or a command inside a script or an ssh line).

/// `s` as one safe shell word: 'it'\''s'.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn quotes_what_the_shell_would_otherwise_interpret() {
        assert_eq!(quote("plain"), "'plain'");
        assert_eq!(quote("it's"), "'it'\\''s'");
        // What comes out of a real shell is exactly what went in.
        for nasty in [
            "a b",
            "x'y",
            "$(echo hacked)",
            "`id`",
            "a;b",
            "*",
            "\"q\"",
            "é\\n",
            "-n",
        ] {
            let out = Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("printf %s {}", quote(nasty)))
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout), nasty, "{nasty}");
        }
    }
}
