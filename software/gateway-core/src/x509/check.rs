//! The decisions of the certificate manager: is a certificate usable as the
//! web server's, what status does the UI show, when is an EST renewal due.
//! Pure functions of the certificate, the device key, the FQDN and what is
//! known about the time.

use super::cert::{split_concatenated, Certificate};
use super::time::DAY;

/// The most certificates a chain may have (leaf included).
pub const CHAIN_MAX: usize = 5;

/// What the gateway knows about the current time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    /// Synchronised: Unix seconds.
    Known(i64),
    /// Not synchronised; the time is at least this (the firmware's build
    /// time), which still rules out certificates that expired before it.
    AtLeast(i64),
}

impl Clock {
    pub fn known(self) -> Option<i64> {
        match self {
            Clock::Known(now) => Some(now),
            Clock::AtLeast(_) => None,
        }
    }

    fn lower_bound(self) -> i64 {
        match self {
            Clock::Known(now) | Clock::AtLeast(now) => now,
        }
    }
}

/// Why a certificate can't be activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// Nothing that parses as a certificate.
    Unparsable,
    /// None of the certificates carries the device's key.
    KeyMismatch,
    /// The certificate isn't for the configured FQDN.
    FqdnMismatch,
    Expired,
    NotYetValid,
    /// More certificates than [`CHAIN_MAX`], or more than fit.
    TooLong,
}

impl Problem {
    /// The machine-readable name the REST API uses.
    pub fn code(self) -> &'static str {
        match self {
            Problem::Unparsable => "unparsable",
            Problem::KeyMismatch => "key_mismatch",
            Problem::FqdnMismatch => "fqdn_mismatch",
            Problem::Expired => "expired",
            Problem::NotYetValid => "not_yet_valid",
            Problem::TooLong => "chain_too_long",
        }
    }
}

/// Does the certificate name `pattern` (a SAN `dNSName`) cover `fqdn`?
/// Case-insensitive; a leading `*.` matches exactly one label.
pub fn name_matches(pattern: &str, fqdn: &str) -> bool {
    let fqdn = fqdn.trim_end_matches('.');
    let pattern = pattern.trim_end_matches('.');
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return match fqdn.split_once('.') {
            Some((label, rest)) => !label.is_empty() && rest.eq_ignore_ascii_case(suffix),
            None => false,
        };
    }
    pattern.eq_ignore_ascii_case(fqdn)
}

/// Is the certificate issued for `fqdn` (by one of its SAN DNS names)?
pub fn covers(cert: &Certificate<'_>, fqdn: &str) -> bool {
    cert.dns_names().any(|name| name_matches(name, fqdn))
}

/// Orders certificates (in any order, e.g. from a PEM upload or an EST
/// PKCS#7) into a chain: the one with the device's key first, then each
/// one's issuer as far as it is there. Certificates that aren't part of the
/// chain are dropped. `device_point` is the uncompressed SEC1 point.
pub fn order_chain<'a>(
    certs: impl Iterator<Item = &'a [u8]>,
    device_point: &[u8],
) -> Result<heapless::Vec<Certificate<'a>, CHAIN_MAX>, Problem> {
    let mut all: heapless::Vec<Certificate<'a>, 8> = heapless::Vec::new();
    for der in certs {
        let cert = Certificate::parse(der).ok_or(Problem::Unparsable)?;
        all.push(cert).map_err(|_| Problem::TooLong)?;
    }
    if all.is_empty() {
        return Err(Problem::Unparsable);
    }
    let leaf = *all
        .iter()
        .find(|c| c.public_key == device_point)
        .ok_or(Problem::KeyMismatch)?;

    let mut chain = heapless::Vec::new();
    let _ = chain.push(leaf);
    let mut current = leaf;
    while !current.is_self_issued() {
        let Some(issuer) = all
            .iter()
            .find(|c| c.subject == current.issuer && c.der != current.der)
        else {
            break;
        };
        if chain.iter().any(|c: &Certificate<'_>| c.der == issuer.der) {
            break;
        }
        chain.push(*issuer).map_err(|_| Problem::TooLong)?;
        current = *issuer;
    }
    Ok(chain)
}

/// Can `leaf` be the web server's certificate? It must carry the device
/// key, be for `fqdn`, and not be outside its validity as far as the clock
/// tells.
pub fn check_leaf(
    leaf: &Certificate<'_>,
    device_point: &[u8],
    fqdn: &str,
    clock: Clock,
) -> Result<(), Problem> {
    if leaf.public_key != device_point {
        return Err(Problem::KeyMismatch);
    }
    if !covers(leaf, fqdn) {
        return Err(Problem::FqdnMismatch);
    }
    check_dates(leaf, clock)
}

/// Is the certificate within its validity? Without a clock only "expired
/// before the firmware was built" can be told.
pub fn check_dates(cert: &Certificate<'_>, clock: Clock) -> Result<(), Problem> {
    if cert.not_after <= clock.lower_bound() {
        return Err(Problem::Expired);
    }
    // Five minutes of slack for a clock that runs a little behind.
    if let Some(now) = clock.known() {
        if cert.not_before > now + 300 {
            return Err(Problem::NotYetValid);
        }
    }
    Ok(())
}

/// The status the UI shows for the active certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Valid,
    /// Valid, but EST/the user should renew it soon (see [`renewal_due`]).
    ExpiresSoon,
    Expired,
    NotYetValid,
    /// Not issued for the configured FQDN (browsers will warn).
    FqdnMismatch,
    /// Within its validity as far as can be told, but the clock isn't set.
    TimeUnknown,
}

impl Status {
    pub fn code(self) -> &'static str {
        match self {
            Status::Valid => "valid",
            Status::ExpiresSoon => "expires_soon",
            Status::Expired => "expired",
            Status::NotYetValid => "not_yet_valid",
            Status::FqdnMismatch => "fqdn_mismatch",
            Status::TimeUnknown => "time_unknown",
        }
    }
}

pub fn status(leaf: &Certificate<'_>, fqdn: &str, clock: Clock) -> Status {
    match check_dates(leaf, clock) {
        Err(Problem::Expired) => return Status::Expired,
        Err(_) => return Status::NotYetValid,
        Ok(()) => {}
    }
    if !fqdn.is_empty() && !covers(leaf, fqdn) {
        return Status::FqdnMismatch;
    }
    match clock.known() {
        None => Status::TimeUnknown,
        Some(now) if renewal_due(leaf.not_before, leaf.not_after, now) => Status::ExpiresSoon,
        Some(_) => Status::Valid,
    }
}

/// Renewal is due 30 days before `not_after`, or after two thirds of the
/// lifetime for a certificate that lives less than 90 days.
pub fn renewal_due(not_before: i64, not_after: i64, now: i64) -> bool {
    let lifetime = (not_after - not_before).max(0);
    let margin = (30 * DAY).min(lifetime / 3);
    now >= not_after - margin
}

/// Seconds to wait after the `failures`-th failed EST attempt in a row
/// (1 = the first failure): one minute, doubled each time, at most six
/// hours.
pub fn backoff(failures: u32) -> u32 {
    let exponent = failures.saturating_sub(1).min(16);
    (60u32 << exponent).min(6 * 3600)
}

/// The leaf of a stored chain (concatenated DER, leaf first).
pub fn stored_leaf(chain: &[u8]) -> Option<Certificate<'_>> {
    split_concatenated(chain)
        .next()
        .and_then(Certificate::parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x509::fixtures::*;
    use crate::x509::pkcs7;

    fn leaf_and_ca() -> (std::vec::Vec<u8>, std::vec::Vec<u8>) {
        (unhex(LEAF), unhex(CA))
    }

    #[test]
    fn wildcards_match_one_label() {
        assert!(name_matches("gw.example.com", "GW.Example.com"));
        assert!(name_matches("*.example.com", "gw.example.com"));
        assert!(!name_matches("*.example.com", "a.gw.example.com"));
        assert!(!name_matches("*.example.com", "example.com"));
        assert!(!name_matches("gw.example.com", "gw2.example.com"));
        assert!(name_matches("gw.example.com.", "gw.example.com"));
    }

    #[test]
    fn orders_a_chain_given_in_any_order() {
        let (leaf, ca) = leaf_and_ca();
        let point = unhex(LEAF_POINT);
        let chain = order_chain([ca.as_slice(), leaf.as_slice()].into_iter(), &point).unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].der, leaf.as_slice());
        assert_eq!(chain[1].der, ca.as_slice());

        // From the EST PKCS#7, where the CA comes first.
        let p7 = unhex(P7_CERTS_ONLY);
        let chain = order_chain(pkcs7::certificates(&p7).unwrap(), &point).unwrap();
        assert_eq!(chain[0].der, leaf.as_slice());
        assert_eq!(chain.len(), 2);
    }

    #[test]
    fn a_chain_without_the_device_key_is_refused() {
        let (_, ca) = leaf_and_ca();
        let point = unhex(LEAF_POINT);
        assert_eq!(
            order_chain([ca.as_slice()].into_iter(), &point).err(),
            Some(Problem::KeyMismatch)
        );
        assert_eq!(
            order_chain([&[0x30, 0x00][..]].into_iter(), &point).err(),
            Some(Problem::Unparsable)
        );
        assert_eq!(
            order_chain(core::iter::empty(), &point).err(),
            Some(Problem::Unparsable)
        );
    }

    #[test]
    fn leaf_checks() {
        let (leaf, _) = leaf_and_ca();
        let point = unhex(LEAF_POINT);
        let cert = Certificate::parse(&leaf).unwrap();
        let inside = Clock::Known(LEAF_NOT_BEFORE + DAY);
        assert_eq!(check_leaf(&cert, &point, "gw.example.com", inside), Ok(()));
        assert_eq!(check_leaf(&cert, &point, "alt.example.com", inside), Ok(()));
        assert_eq!(
            check_leaf(&cert, &point, "other.example.com", inside),
            Err(Problem::FqdnMismatch)
        );
        assert_eq!(
            check_leaf(&cert, &[4; 65], "gw.example.com", inside),
            Err(Problem::KeyMismatch)
        );
        assert_eq!(
            check_leaf(
                &cert,
                &point,
                "gw.example.com",
                Clock::Known(LEAF_NOT_AFTER)
            ),
            Err(Problem::Expired)
        );
        assert_eq!(
            check_leaf(
                &cert,
                &point,
                "gw.example.com",
                Clock::Known(LEAF_NOT_BEFORE - DAY)
            ),
            Err(Problem::NotYetValid)
        );
        // Without a clock, only "expired before the build" is known.
        assert_eq!(
            check_leaf(&cert, &point, "gw.example.com", Clock::AtLeast(0)),
            Ok(())
        );
        assert_eq!(
            check_leaf(
                &cert,
                &point,
                "gw.example.com",
                Clock::AtLeast(LEAF_NOT_AFTER + 1)
            ),
            Err(Problem::Expired)
        );
    }

    #[test]
    fn statuses() {
        let (leaf, _) = leaf_and_ca();
        let cert = Certificate::parse(&leaf).unwrap();
        let fqdn = "gw.example.com";
        assert_eq!(
            status(&cert, fqdn, Clock::Known(LEAF_NOT_BEFORE + DAY)),
            Status::Valid
        );
        assert_eq!(
            status(&cert, fqdn, Clock::Known(LEAF_NOT_AFTER - DAY)),
            Status::ExpiresSoon
        );
        assert_eq!(
            status(&cert, fqdn, Clock::Known(LEAF_NOT_AFTER + 1)),
            Status::Expired
        );
        assert_eq!(status(&cert, fqdn, Clock::AtLeast(0)), Status::TimeUnknown);
        assert_eq!(
            status(&cert, "x.example.com", Clock::Known(LEAF_NOT_BEFORE + DAY)),
            Status::FqdnMismatch
        );
    }

    #[test]
    fn renewal_and_backoff() {
        let year = 365 * DAY;
        assert!(!renewal_due(0, year, year - 31 * DAY));
        assert!(renewal_due(0, year, year - 30 * DAY));
        // A 30-day certificate is renewed after 20 days, not right away.
        assert!(!renewal_due(0, 30 * DAY, 19 * DAY));
        assert!(renewal_due(0, 30 * DAY, 20 * DAY));
        assert_eq!(backoff(1), 60);
        assert_eq!(backoff(2), 120);
        assert_eq!(backoff(9), 60 << 8);
        assert_eq!(backoff(10), 6 * 3600);
        assert_eq!(backoff(100), 6 * 3600);
    }

    #[test]
    fn stored_chain_leaf() {
        let (leaf, ca) = leaf_and_ca();
        let mut stored = leaf.clone();
        stored.extend(ca);
        assert_eq!(stored_leaf(&stored).unwrap().der, leaf.as_slice());
        assert!(stored_leaf(&[]).is_none());
    }
}
