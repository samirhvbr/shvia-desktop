//! Strict argv parsing — three subcommands. A parser crate would be overkill for a sidecar whose
//! only caller is `code_bridge::traffic`, but the values it receives come from a web page (the
//! interface id, the capture filter), so the parser never lets one value pose as a flag: each flag
//! takes exactly the next token, whatever it looks like, an unknown or repeated flag is an error,
//! and the interface is validated against the real device list in `main.rs`.

use std::collections::HashMap;

/// The capture file stops growing here; the page is told and the file stays exportable.
pub const DEFAULT_MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Args {
    /// Enumerate local interfaces. Works without elevated capability on every supported OS.
    ListInterfaces,
    /// Try to open an interface briefly, to tell "ready" from "needs permission" apart. Without
    /// `--interface` the first usable device is tried: the capability belongs to the binary, not
    /// to a device.
    CheckPermission { interface: Option<String> },
    /// Open `interface`, apply `filter` (a BPF expression) if given, and stream frames.
    Capture { interface: String, filter: Option<String>, out: String, max_bytes: u64 },
}

impl Args {
    pub fn parse(argv: &[String]) -> Result<Self, String> {
        let (command, rest) = argv.split_first().ok_or("missing subcommand")?;
        match command.as_str() {
            "--list-interfaces" => {
                if rest.is_empty() { Ok(Self::ListInterfaces) } else { Err("--list-interfaces takes no arguments".into()) }
            }
            "--check-permission" => {
                let mut flags = flags(rest, &["--interface"])?;
                Ok(Self::CheckPermission { interface: flags.remove("--interface").filter(|i| !i.is_empty()) })
            }
            "--capture" => {
                let mut flags = flags(rest, &["--interface", "--filter", "--out", "--max-bytes"])?;
                let interface = flags.remove("--interface").ok_or("--interface is required")?;
                let out = flags.remove("--out").ok_or("--out is required")?;
                let filter = flags.remove("--filter").filter(|f| !f.is_empty());
                let max_bytes = match flags.remove("--max-bytes") {
                    None => DEFAULT_MAX_BYTES,
                    Some(n) => n.parse::<u64>().ok().filter(|n| *n > 0).ok_or("--max-bytes must be a positive number")?,
                };
                Ok(Self::Capture { interface, filter, out, max_bytes })
            }
            other => Err(format!("unknown subcommand: {other:?}")),
        }
    }
}

fn flags(rest: &[String], allowed: &[&str]) -> Result<HashMap<String, String>, String> {
    let mut found = HashMap::new();
    let mut tokens = rest.iter();
    while let Some(flag) = tokens.next() {
        if !allowed.contains(&flag.as_str()) {
            return Err(format!("unexpected argument {flag:?}"));
        }
        let value = tokens.next().ok_or_else(|| format!("{flag} needs a value"))?;
        if found.insert(flag.clone(), value.clone()).is_some() {
            return Err(format!("{flag} given twice"));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn list_interfaces_takes_no_arguments() {
        assert_eq!(Args::parse(&v(&["--list-interfaces"])), Ok(Args::ListInterfaces));
        assert!(Args::parse(&v(&["--list-interfaces", "x"])).is_err());
    }

    #[test]
    fn the_permission_probe_works_with_or_without_an_interface() {
        assert_eq!(Args::parse(&v(&["--check-permission"])), Ok(Args::CheckPermission { interface: None }));
        assert_eq!(
            Args::parse(&v(&["--check-permission", "--interface", "eth0"])),
            Ok(Args::CheckPermission { interface: Some("eth0".into()) })
        );
    }

    #[test]
    fn capture_requires_interface_and_out_and_defaults_the_size_cap() {
        assert!(Args::parse(&v(&["--capture", "--interface", "eth0"])).is_err());
        let parsed = Args::parse(&v(&["--capture", "--interface", "eth0", "--out", "/tmp/a.pcapng"]));
        assert_eq!(
            parsed,
            Ok(Args::Capture {
                interface: "eth0".into(),
                filter: None,
                out: "/tmp/a.pcapng".into(),
                max_bytes: DEFAULT_MAX_BYTES
            })
        );
    }

    #[test]
    fn an_empty_filter_is_the_same_as_no_filter() {
        let parsed = Args::parse(&v(&["--capture", "--interface", "eth0", "--out", "/tmp/a", "--filter", ""]));
        assert!(matches!(parsed, Ok(Args::Capture { filter: None, .. })));
    }

    #[test]
    fn a_value_that_looks_like_a_flag_stays_a_value() {
        // The page names the interface. An interface called "--out" must not be able to steal the
        // `--out` slot and make the sidecar create a file named after the next flag.
        let parsed = Args::parse(&v(&[
            "--capture", "--interface", "--out", "--filter", "tcp", "--out", "/tmp/a.pcapng",
        ]));
        assert_eq!(
            parsed,
            Ok(Args::Capture {
                interface: "--out".into(),
                filter: Some("tcp".into()),
                out: "/tmp/a.pcapng".into(),
                max_bytes: DEFAULT_MAX_BYTES
            })
        );
    }

    #[test]
    fn unknown_repeated_and_dangling_flags_are_refused() {
        assert!(Args::parse(&v(&["--capture", "--interface", "a", "--out", "b", "--bogus", "c"])).is_err());
        assert!(Args::parse(&v(&["--capture", "--interface", "a", "--interface", "b", "--out", "c"])).is_err());
        assert!(Args::parse(&v(&["--capture", "--interface", "a", "--out"])).is_err());
        assert!(Args::parse(&v(&["--capture", "--interface", "a", "--out", "b", "--max-bytes", "0"])).is_err());
        assert!(Args::parse(&v(&["--capture", "--interface", "a", "--out", "b", "--max-bytes", "x"])).is_err());
        assert!(Args::parse(&v(&[])).is_err());
    }
}
