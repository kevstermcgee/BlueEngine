//! Typed server addresses and the default-server chain, graphics-free.
//!
//! A player types or pastes `play.example.com`, `203.0.113.5:27015` or `localhost`; [`resolve_ipv4`] turns it
//! into a [`SocketAddr`] or an [`AddressError`] whose `Display` is a sentence for the player. Netplay servers
//! bind `0.0.0.0` (IPv4 only), so an IPv6 literal, or a name that only has IPv6 records, is refused with a
//! message instead of a silent timeout.
//!
//! Name lookup blocks (a mistyped name can take seconds on a bad network). Call it when the player presses
//! Connect, or from a worker thread, not every frame. [`resolve_ipv4_with`] takes the lookup as a closure so
//! tests do not touch the network.
//!
//! [`ServerChoice::first_of`] is the default-server chain every online game wants: a `--connect` flag, else a
//! `server.txt` beside the program, else the address the player used last time, else one built into the game.
use std::fmt;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs};
use std::path::Path;

/// Why an address could not be used. `Display` is player-facing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddressError {
    /// Nothing typed.
    Empty,
    /// An IPv6 literal such as `[::1]:27015` or `fe80::1`.
    Ipv6Unsupported,
    /// The port is missing after `:`, not a number, or outside 1-65535.
    BadPort(String),
    /// The host is neither an IPv4 address nor a plausible name (the text is what was given).
    BadHost(String),
    /// The name has no address at all.
    NotFound(String),
    /// The name only resolves to IPv6 addresses.
    NoIpv4(String),
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressError::Empty => write!(f, "Type the server's address, like 203.0.113.5:27015."),
            AddressError::Ipv6Unsupported => write!(
                f,
                "IPv6 addresses are not supported. Use the server's IPv4 address (like 203.0.113.5:27015) or its name."
            ),
            AddressError::BadPort(p) if p.is_empty() => {
                write!(f, "The port is missing after the colon. Use a number from 1 to 65535, or leave the colon out.")
            }
            AddressError::BadPort(p) => {
                write!(f, "\"{p}\" is not a valid port. Use a number from 1 to 65535.")
            }
            AddressError::BadHost(h) => write!(
                f,
                "\"{h}\" is not a valid server address. Use an IPv4 address like 203.0.113.5 or a name like play.example.com."
            ),
            AddressError::NotFound(h) => write!(
                f,
                "Could not find a server named \"{h}\". Check the spelling and your internet connection, or use its IP address."
            ),
            AddressError::NoIpv4(h) => write!(
                f,
                "\"{h}\" has no IPv4 address, and servers listen on IPv4 only. Ask the host for the IPv4 address."
            ),
        }
    }
}

impl std::error::Error for AddressError {}

/// [`resolve_ipv4_with`] using the operating system's resolver.
pub fn resolve_ipv4(input: &str, default_port: u16) -> Result<SocketAddr, AddressError> {
    resolve_ipv4_with(input, default_port, |host, port| {
        (host, port).to_socket_addrs().map(|it| it.collect())
    })
}

/// Accepts `host`, `host:port` and `ip:port` (whitespace around it is ignored), uses `default_port` when none
/// is given, and returns an IPv4 socket address. A literal IPv4 never calls `lookup`; a name calls it once
/// with `(host, port)` and the first IPv4 answer wins.
pub fn resolve_ipv4_with(
    input: &str,
    default_port: u16,
    lookup: impl FnOnce(&str, u16) -> std::io::Result<Vec<SocketAddr>>,
) -> Result<SocketAddr, AddressError> {
    let text = input.trim();
    if text.is_empty() {
        return Err(AddressError::Empty);
    }
    // Brackets only exist in IPv6 syntax, and a second colon means an unbracketed IPv6 literal.
    if text.contains('[') || text.contains(']') || text.matches(':').count() > 1 {
        return Err(AddressError::Ipv6Unsupported);
    }
    let (host, port) = match text.split_once(':') {
        Some((host, port)) => {
            let port = port.trim();
            let parsed = port.parse::<u16>().ok().filter(|p| *p != 0);
            (
                host.trim(),
                parsed.ok_or_else(|| AddressError::BadPort(port.to_string()))?,
            )
        }
        None => (text, default_port),
    };
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(SocketAddr::V4(SocketAddrV4::new(ip, port)));
    }
    if !plausible_host(host) {
        return Err(AddressError::BadHost(host.to_string()));
    }
    let answers = lookup(host, port).map_err(|_| AddressError::NotFound(host.to_string()))?;
    if answers.is_empty() {
        return Err(AddressError::NotFound(host.to_string()));
    }
    answers
        .into_iter()
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| AddressError::NoIpv4(host.to_string()))
}

/// A name the resolver should be asked about: letters, digits, `-` and `_`, in dot-separated labels, and not a
/// number-looking string such as `300.1.1.1` or `1.2.3` (those are typos, not names).
fn plausible_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    let numeric_looking = host.chars().all(|c| c.is_ascii_digit() || c == '.');
    if numeric_looking {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

/// One place a default server address may come from, tried in the order the game lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerSource<'a> {
    /// A command-line value (`flag_value(&args, "--connect")`); `None` when the flag is absent.
    CliArg(Option<&'a str>),
    /// A file of this name in the directory given to [`ServerChoice::first_of`]: first line `host:port`,
    /// optional second line the join key (`-` for none); blank lines and `#` comments are skipped.
    FileBesideExe(&'a str),
    /// The address the player used last time (`Settings::last_server`).
    LastUsed(Option<&'a str>),
    /// The address built into the game.
    Builtin(&'a str),
}

/// Which [`ServerSource`] a [`ServerChoice`] came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerOrigin {
    CliArg,
    File,
    LastUsed,
    Builtin,
}

/// The default server: unresolved text (resolve it with [`resolve_ipv4`] when the player connects).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerChoice {
    pub address: String,
    /// The join key from the second line of a server file, when it had one.
    pub join_key: Option<String>,
    pub origin: ServerOrigin,
}

impl ServerChoice {
    /// The first source that yields a non-empty address, in the order given. `dir` is where
    /// `FileBesideExe` files are read from (an explicit directory keeps tests deterministic; see
    /// [`ServerChoice::first_of_beside_exe`]). A missing or unreadable file is just skipped.
    pub fn first_of(dir: &Path, sources: &[ServerSource]) -> Option<ServerChoice> {
        let plain = |text: &str, origin| {
            let text = text.trim();
            (!text.is_empty()).then(|| ServerChoice {
                address: text.to_string(),
                join_key: None,
                origin,
            })
        };
        sources.iter().find_map(|source| match *source {
            ServerSource::CliArg(value) => value.and_then(|v| plain(v, ServerOrigin::CliArg)),
            ServerSource::LastUsed(value) => value.and_then(|v| plain(v, ServerOrigin::LastUsed)),
            ServerSource::Builtin(value) => plain(value, ServerOrigin::Builtin),
            ServerSource::FileBesideExe(name) => {
                parse_server_file(&std::fs::read_to_string(dir.join(name)).ok()?)
            }
        })
    }

    /// [`ServerChoice::first_of`] reading files from the directory of the running executable (the working
    /// directory when that is unknown).
    pub fn first_of_beside_exe(sources: &[ServerSource]) -> Option<ServerChoice> {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        Self::first_of(&dir, sources)
    }
}

/// The contents of a `server.txt`: first meaningful line is the address, the second an optional join key.
fn parse_server_file(text: &str) -> Option<ServerChoice> {
    let text = text.trim_start_matches('\u{feff}'); // a Windows editor's byte order mark
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'));
    let address = lines.next()?.to_string();
    let join_key = lines.next().filter(|k| *k != "-").map(str::to_string);
    Some(ServerChoice {
        address,
        join_key,
        origin: ServerOrigin::File,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn no_lookup(_: &str, _: u16) -> io::Result<Vec<SocketAddr>> {
        panic!("a literal IPv4 or a bad address must not be looked up")
    }

    fn resolve(input: &str) -> Result<SocketAddr, AddressError> {
        resolve_ipv4_with(input, 27015, no_lookup)
    }

    #[test]
    fn ip_literals_with_and_without_a_port() {
        assert_eq!(
            resolve("203.0.113.5:4000").unwrap(),
            "203.0.113.5:4000".parse().unwrap()
        );
        assert_eq!(
            resolve("203.0.113.5").unwrap(),
            "203.0.113.5:27015".parse().unwrap(),
            "missing port: the default"
        );
        assert_eq!(
            resolve("  \t127.0.0.1 : 99 \n").unwrap(),
            "127.0.0.1:99".parse().unwrap(),
            "whitespace is trimmed"
        );
        assert_eq!(resolve("0.0.0.0:1").unwrap().port(), 1);
    }

    #[test]
    fn ipv6_is_refused_with_a_clear_message() {
        for text in [
            "[::1]:27015",
            "::1",
            "fe80::1",
            "[2001:db8::1]",
            "2001:db8::1:27015",
        ] {
            let err = resolve(text).unwrap_err();
            assert_eq!(err, AddressError::Ipv6Unsupported, "{text}");
            assert!(err.to_string().contains("IPv4"), "{err}");
        }
    }

    #[test]
    fn bad_input_gets_a_specific_error() {
        assert_eq!(resolve("").unwrap_err(), AddressError::Empty);
        assert_eq!(resolve("  \n").unwrap_err(), AddressError::Empty);
        assert_eq!(
            resolve("1.2.3.4:").unwrap_err(),
            AddressError::BadPort(String::new())
        );
        assert_eq!(
            resolve("1.2.3.4:0").unwrap_err(),
            AddressError::BadPort("0".into())
        );
        assert_eq!(
            resolve("1.2.3.4:65536").unwrap_err(),
            AddressError::BadPort("65536".into())
        );
        assert_eq!(
            resolve("1.2.3.4:abc").unwrap_err(),
            AddressError::BadPort("abc".into())
        );
        assert_eq!(
            resolve("1.2.3.4:-1").unwrap_err(),
            AddressError::BadPort("-1".into())
        );
        assert_eq!(
            resolve(":27015").unwrap_err(),
            AddressError::BadHost(String::new())
        );
        for typo in [
            "999.1.1.1",
            "1.2.3",
            "300.300.300.300:5",
            "a..b",
            "-bad.example",
            "bad-.example",
            "has space.com",
            "a/b",
            "ex!ample.com",
        ] {
            assert!(
                matches!(resolve(typo), Err(AddressError::BadHost(_))),
                "{typo}: {:?}",
                resolve(typo)
            );
        }
        assert!(
            matches!(resolve(&"a".repeat(64)), Err(AddressError::BadHost(_))),
            "a 64-character label"
        );
        for err in [
            AddressError::BadHost("x".into()),
            AddressError::BadPort("0".into()),
            AddressError::Empty,
        ] {
            assert!(!err.to_string().is_empty());
        }
    }

    #[test]
    fn names_resolve_through_the_lookup_and_prefer_ipv4() {
        let seen = std::cell::RefCell::new(None);
        let got = resolve_ipv4_with("Play.Example.com:5000", 1, |host, port| {
            *seen.borrow_mut() = Some((host.to_string(), port));
            Ok(vec![
                "[::1]:5000".parse().unwrap(),
                "192.0.2.7:5000".parse().unwrap(),
            ])
        });
        assert_eq!(got.unwrap(), "192.0.2.7:5000".parse().unwrap());
        assert_eq!(
            seen.into_inner(),
            Some(("Play.Example.com".to_string(), 5000))
        );

        let default_port = resolve_ipv4_with("host_name", 27015, |_, port| {
            Ok(vec![format!("192.0.2.9:{port}").parse().unwrap()])
        });
        assert_eq!(
            default_port.unwrap().port(),
            27015,
            "the default port reaches the lookup"
        );

        let only6 = resolve_ipv4_with("v6.example", 1, |_, _| Ok(vec!["[::1]:1".parse().unwrap()]));
        assert_eq!(
            only6.unwrap_err(),
            AddressError::NoIpv4("v6.example".into())
        );
        let nothing = resolve_ipv4_with("gone.example", 1, |_, _| Ok(vec![]));
        assert_eq!(
            nothing.unwrap_err(),
            AddressError::NotFound("gone.example".into())
        );
        let failing = resolve_ipv4_with("down.example", 1, |_, _| Err(io::Error::other("no dns")));
        let err = failing.unwrap_err();
        assert_eq!(err, AddressError::NotFound("down.example".into()));
        assert!(err.to_string().contains("down.example"));
    }

    #[test]
    fn localhost_resolves_with_the_real_resolver_or_fails_politely() {
        // Every desktop OS maps localhost through its hosts file; a locked-down sandbox may not.
        match resolve_ipv4("localhost:4242", 1) {
            Ok(addr) => assert!(
                addr.is_ipv4() && addr.ip().is_loopback() && addr.port() == 4242,
                "{addr}"
            ),
            Err(e) => assert!(
                matches!(e, AddressError::NotFound(_) | AddressError::NoIpv4(_)),
                "{e:?}"
            ),
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("net_address_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_first_non_empty_source_wins_in_the_order_given() {
        let dir = temp_dir("order");
        std::fs::write(
            dir.join("server.txt"),
            "# comment\n\n  files.example:1234  \nsecret\n",
        )
        .unwrap();
        let order = |cli, last| {
            ServerChoice::first_of(
                &dir,
                &[
                    ServerSource::CliArg(cli),
                    ServerSource::FileBesideExe("server.txt"),
                    ServerSource::LastUsed(last),
                    ServerSource::Builtin("built.in:1"),
                ],
            )
            .unwrap()
        };
        let c = order(Some(" cli.example:9 "), Some("last:2"));
        assert_eq!(
            (c.address.as_str(), c.origin, c.join_key),
            ("cli.example:9", ServerOrigin::CliArg, None)
        );
        let c = order(Some("   "), Some("last:2"));
        assert_eq!(
            (c.address.as_str(), c.origin),
            ("files.example:1234", ServerOrigin::File),
            "a blank flag is skipped"
        );
        assert_eq!(c.join_key.as_deref(), Some("secret"));
        std::fs::remove_file(dir.join("server.txt")).unwrap();
        let c = order(None, Some("last:2"));
        assert_eq!(
            (c.address.as_str(), c.origin),
            ("last:2", ServerOrigin::LastUsed),
            "no file: last used"
        );
        let c = order(None, Some(""));
        assert_eq!(
            (c.address.as_str(), c.origin),
            ("built.in:1", ServerOrigin::Builtin)
        );
        assert_eq!(
            ServerChoice::first_of(
                &dir,
                &[ServerSource::CliArg(None), ServerSource::Builtin("")]
            ),
            None
        );
        assert_eq!(ServerChoice::first_of(&dir, &[]), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn server_files_tolerate_comments_a_bom_dash_keys_and_empty_files() {
        let dir = temp_dir("files");
        let read = |text: &str| {
            std::fs::write(dir.join("server.txt"), text).unwrap();
            ServerChoice::first_of(&dir, &[ServerSource::FileBesideExe("server.txt")])
        };
        let c = read("\u{feff}1.2.3.4:5\r\n-\r\ndevelopment\r\n").unwrap();
        assert_eq!(
            (c.address.as_str(), c.join_key),
            ("1.2.3.4:5", None),
            "BOM, CRLF and a dash for no key"
        );
        assert_eq!(read("").map(|c| c.address), None);
        assert_eq!(read("# only comments\n   \n").map(|c| c.address), None);
        assert_eq!(read("host").map(|c| c.address), Some("host".into()));
        assert_eq!(
            ServerChoice::first_of(&dir, &[ServerSource::FileBesideExe("absent.txt")]),
            None,
            "a missing file is not an error"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
