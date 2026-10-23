//! Minimal argv parsing — three subcommands, no flags beyond what each one needs. A real
//! parser crate would be overkill for a sidecar whose only caller is `code_bridge::traffic`,
//! which always builds the argv itself; nothing here reads untrusted shell input.

pub enum Args {
    /// Enumerate local interfaces. Works without elevated capability on every supported OS.
    ListInterfaces,
    /// Try to open `interface` briefly, to tell "ready" from "needs permission" apart.
    CheckPermission { interface: String },
    /// Open `interface`, apply `filter` (a BPF expression) if given, and stream frames.
    Capture { interface: String, filter: Option<String>, out: String },
}

impl Args {
    pub fn parse(argv: &[String]) -> Result<Self, String> {
        match argv.first().map(String::as_str) {
            Some("--list-interfaces") => Ok(Self::ListInterfaces),
            Some("--check-permission") => {
                let interface = value_of(argv, "--interface").ok_or("--interface is required")?;
                Ok(Self::CheckPermission { interface })
            }
            Some("--capture") => {
                let interface = value_of(argv, "--interface").ok_or("--interface is required")?;
                let out = value_of(argv, "--out").ok_or("--out is required")?;
                let filter = value_of(argv, "--filter").filter(|f| !f.is_empty());
                Ok(Self::Capture { interface, filter, out })
            }
            other => Err(format!("unknown or missing subcommand: {other:?}")),
        }
    }
}

fn value_of(argv: &[String], flag: &str) -> Option<String> {
    argv.iter().position(|a| a == flag).and_then(|i| argv.get(i + 1)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn list_interfaces_takes_no_arguments() {
        assert!(matches!(Args::parse(&v(&["--list-interfaces"])), Ok(Args::ListInterfaces)));
    }

    #[test]
    fn capture_requires_interface_and_out() {
        assert!(Args::parse(&v(&["--capture", "--interface", "eth0"])).is_err());
        let parsed = Args::parse(&v(&["--capture", "--interface", "eth0", "--out", "/tmp/a.pcapng"]));
        let Ok(Args::Capture { interface, filter, out }) = parsed else { panic!("expected Capture") };
        assert_eq!(interface, "eth0");
        assert_eq!(out, "/tmp/a.pcapng");
        assert!(filter.is_none());
    }

    #[test]
    fn an_empty_filter_is_the_same_as_no_filter() {
        let parsed = Args::parse(&v(&[
            "--capture", "--interface", "eth0", "--out", "/tmp/a.pcapng", "--filter", "",
        ]));
        let Ok(Args::Capture { filter, .. }) = parsed else { panic!("expected Capture") };
        assert!(filter.is_none());
    }
}
