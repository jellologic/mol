// SPDX-License-Identifier: MIT OR Apache-2.0
//! Darwin errno numbers, names, and the Linux→Darwin map (doc 11 F1).
//!
//! Darwin values from the macOS SDK `sys/errno.h` (`ELAST = 107`);
//! Linux values from `asm-generic/errno-base.h` + `errno.h`
//! (1..=133). `from_linux` maps by NAME: the 1:1 range first, then
//! each renumber (notably the `EAGAIN`/`EDEADLK` 11/35 swap), then a
//! closest-cousin fallback for Linux-only codes. A pass-through
//! default would be wrong (Linux 11 is `EAGAIN`, Darwin 11 is
//! `EDEADLK`), so unknown inputs map to `EIO`.

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const ESRCH: i32 = 3;
pub const EINTR: i32 = 4;
pub const EIO: i32 = 5;
pub const ENXIO: i32 = 6;
pub const E2BIG: i32 = 7;
pub const ENOEXEC: i32 = 8;
pub const EBADF: i32 = 9;
pub const ECHILD: i32 = 10;
pub const EDEADLK: i32 = 11;
pub const ENOMEM: i32 = 12;
pub const EACCES: i32 = 13;
pub const EFAULT: i32 = 14;
pub const ENOTBLK: i32 = 15;
pub const EBUSY: i32 = 16;
pub const EEXIST: i32 = 17;
pub const EXDEV: i32 = 18;
pub const ENODEV: i32 = 19;
pub const ENOTDIR: i32 = 20;
pub const EISDIR: i32 = 21;
pub const EINVAL: i32 = 22;
pub const ENFILE: i32 = 23;
pub const EMFILE: i32 = 24;
pub const ENOTTY: i32 = 25;
pub const ETXTBSY: i32 = 26;
pub const EFBIG: i32 = 27;
pub const ENOSPC: i32 = 28;
pub const ESPIPE: i32 = 29;
pub const EROFS: i32 = 30;
pub const EMLINK: i32 = 31;
pub const EPIPE: i32 = 32;
pub const EDOM: i32 = 33;
pub const ERANGE: i32 = 34;
pub const EAGAIN: i32 = 35;
pub const EINPROGRESS: i32 = 36;
pub const EALREADY: i32 = 37;
pub const ENOTSOCK: i32 = 38;
pub const EDESTADDRREQ: i32 = 39;
pub const EMSGSIZE: i32 = 40;
pub const EPROTOTYPE: i32 = 41;
pub const ENOPROTOOPT: i32 = 42;
pub const EPROTONOSUPPORT: i32 = 43;
pub const ESOCKTNOSUPPORT: i32 = 44;
pub const ENOTSUP: i32 = 45;
pub const EPFNOSUPPORT: i32 = 46;
pub const EAFNOSUPPORT: i32 = 47;
pub const EADDRINUSE: i32 = 48;
pub const EADDRNOTAVAIL: i32 = 49;
pub const ENETDOWN: i32 = 50;
pub const ENETUNREACH: i32 = 51;
pub const ENETRESET: i32 = 52;
pub const ECONNABORTED: i32 = 53;
pub const ECONNRESET: i32 = 54;
pub const ENOBUFS: i32 = 55;
pub const EISCONN: i32 = 56;
pub const ENOTCONN: i32 = 57;
pub const ESHUTDOWN: i32 = 58;
pub const ETOOMANYREFS: i32 = 59;
pub const ETIMEDOUT: i32 = 60;
pub const ECONNREFUSED: i32 = 61;
pub const ELOOP: i32 = 62;
pub const ENAMETOOLONG: i32 = 63;
pub const EHOSTDOWN: i32 = 64;
pub const EHOSTUNREACH: i32 = 65;
pub const ENOTEMPTY: i32 = 66;
pub const EPROCLIM: i32 = 67;
pub const EUSERS: i32 = 68;
pub const EDQUOT: i32 = 69;
pub const ESTALE: i32 = 70;
pub const EREMOTE: i32 = 71;
pub const EBADRPC: i32 = 72;
pub const ERPCMISMATCH: i32 = 73;
pub const EPROGUNAVAIL: i32 = 74;
pub const EPROGMISMATCH: i32 = 75;
pub const EPROCUNAVAIL: i32 = 76;
pub const ENOLCK: i32 = 77;
pub const ENOSYS: i32 = 78;
pub const EFTYPE: i32 = 79;
pub const EAUTH: i32 = 80;
pub const ENEEDAUTH: i32 = 81;
pub const EPWROFF: i32 = 82;
pub const EDEVERR: i32 = 83;
pub const EOVERFLOW: i32 = 84;
pub const EBADEXEC: i32 = 85;
pub const EBADARCH: i32 = 86;
pub const ESHLIBVERS: i32 = 87;
pub const EBADMACHO: i32 = 88;
pub const ECANCELED: i32 = 89;
pub const EIDRM: i32 = 90;
pub const ENOMSG: i32 = 91;
pub const EILSEQ: i32 = 92;
pub const ENOATTR: i32 = 93;
pub const EBADMSG: i32 = 94;
pub const EMULTIHOP: i32 = 95;
pub const ENODATA: i32 = 96;
pub const ENOLINK: i32 = 97;
pub const ENOSR: i32 = 98;
pub const ENOSTR: i32 = 99;
pub const EPROTO: i32 = 100;
pub const ETIME: i32 = 101;
pub const EOPNOTSUPP: i32 = 102;
pub const ENOPOLICY: i32 = 103;
pub const ENOTRECOVERABLE: i32 = 104;
pub const EOWNERDEAD: i32 = 105;
pub const EQFULL: i32 = 106;
pub const ENOTCAPABLE: i32 = 107;
pub const ELAST: i32 = 107;

/// Darwin errno name for `name`-based assertions (P1.2.2: guests see
/// Mac errno names). Aliases (`EWOULDBLOCK`, `EDEADLOCK`) resolve to
/// the canonical name.
pub fn name(no: i32) -> Option<&'static str> {
    Some(match no {
        1 => "EPERM",
        2 => "ENOENT",
        3 => "ESRCH",
        4 => "EINTR",
        5 => "EIO",
        6 => "ENXIO",
        7 => "E2BIG",
        8 => "ENOEXEC",
        9 => "EBADF",
        10 => "ECHILD",
        11 => "EDEADLK",
        12 => "ENOMEM",
        13 => "EACCES",
        14 => "EFAULT",
        15 => "ENOTBLK",
        16 => "EBUSY",
        17 => "EEXIST",
        18 => "EXDEV",
        19 => "ENODEV",
        20 => "ENOTDIR",
        21 => "EISDIR",
        22 => "EINVAL",
        23 => "ENFILE",
        24 => "EMFILE",
        25 => "ENOTTY",
        26 => "ETXTBSY",
        27 => "EFBIG",
        28 => "ENOSPC",
        29 => "ESPIPE",
        30 => "EROFS",
        31 => "EMLINK",
        32 => "EPIPE",
        33 => "EDOM",
        34 => "ERANGE",
        35 => "EAGAIN",
        36 => "EINPROGRESS",
        37 => "EALREADY",
        38 => "ENOTSOCK",
        39 => "EDESTADDRREQ",
        40 => "EMSGSIZE",
        41 => "EPROTOTYPE",
        42 => "ENOPROTOOPT",
        43 => "EPROTONOSUPPORT",
        44 => "ESOCKTNOSUPPORT",
        45 => "ENOTSUP",
        46 => "EPFNOSUPPORT",
        47 => "EAFNOSUPPORT",
        48 => "EADDRINUSE",
        49 => "EADDRNOTAVAIL",
        50 => "ENETDOWN",
        51 => "ENETUNREACH",
        52 => "ENETRESET",
        53 => "ECONNABORTED",
        54 => "ECONNRESET",
        55 => "ENOBUFS",
        56 => "EISCONN",
        57 => "ENOTCONN",
        58 => "ESHUTDOWN",
        59 => "ETOOMANYREFS",
        60 => "ETIMEDOUT",
        61 => "ECONNREFUSED",
        62 => "ELOOP",
        63 => "ENAMETOOLONG",
        64 => "EHOSTDOWN",
        65 => "EHOSTUNREACH",
        66 => "ENOTEMPTY",
        67 => "EPROCLIM",
        68 => "EUSERS",
        69 => "EDQUOT",
        70 => "ESTALE",
        71 => "EREMOTE",
        72 => "EBADRPC",
        73 => "ERPCMISMATCH",
        74 => "EPROGUNAVAIL",
        75 => "EPROGMISMATCH",
        76 => "EPROCUNAVAIL",
        77 => "ENOLCK",
        78 => "ENOSYS",
        79 => "EFTYPE",
        80 => "EAUTH",
        81 => "ENEEDAUTH",
        82 => "EPWROFF",
        83 => "EDEVERR",
        84 => "EOVERFLOW",
        85 => "EBADEXEC",
        86 => "EBADARCH",
        87 => "ESHLIBVERS",
        88 => "EBADMACHO",
        89 => "ECANCELED",
        90 => "EIDRM",
        91 => "ENOMSG",
        92 => "EILSEQ",
        93 => "ENOATTR",
        94 => "EBADMSG",
        95 => "EMULTIHOP",
        96 => "ENODATA",
        97 => "ENOLINK",
        98 => "ENOSR",
        99 => "ENOSTR",
        100 => "EPROTO",
        101 => "ETIME",
        102 => "EOPNOTSUPP",
        103 => "ENOPOLICY",
        104 => "ENOTRECOVERABLE",
        105 => "EOWNERDEAD",
        106 => "EQFULL",
        107 => "ENOTCAPABLE",
        _ => return None,
    })
}

/// Map a Linux errno to the same-named Darwin errno. Linux-only
/// codes map to a documented closest cousin; unknown inputs map to
/// `EIO` (never pass-through: shared numbers with different names
/// would silently misreport).
pub fn from_linux(no: i32) -> i32 {
    match no {
        1 => EPERM,
        2 => ENOENT,
        3 => ESRCH,
        4 => EINTR,
        5 => EIO,
        6 => ENXIO,
        7 => E2BIG,
        8 => ENOEXEC,
        9 => EBADF,
        10 => ECHILD,
        11 => EAGAIN, // swap: Linux EAGAIN=11, Darwin EAGAIN=35
        12 => ENOMEM,
        13 => EACCES,
        14 => EFAULT,
        15 => ENOTBLK,
        16 => EBUSY,
        17 => EEXIST,
        18 => EXDEV,
        19 => ENODEV,
        20 => ENOTDIR,
        21 => EISDIR,
        22 => EINVAL,
        23 => ENFILE,
        24 => EMFILE,
        25 => ENOTTY,
        26 => ETXTBSY,
        27 => EFBIG,
        28 => ENOSPC,
        29 => ESPIPE,
        30 => EROFS,
        31 => EMLINK,
        32 => EPIPE,
        33 => EDOM,
        34 => ERANGE,
        35 => EDEADLK, // swap: Linux EDEADLK=35, Darwin EDEADLK=11
        36 => ENAMETOOLONG,
        37 => ENOLCK,
        38 => ENOSYS,
        39 => ENOTEMPTY,
        40 => ELOOP,
        // 41: unused on Linux.
        42 => ENOMSG,
        43 => EIDRM,
        44 => ENXIO,  // ECHRNG: channel/device cousin
        45 => EPROTO, // EL2NSYNC: streams protocol cousin
        46 => EPROTO, // EL3HLT
        47 => EPROTO, // EL3RST
        48 => EPROTO, // ELNRNG
        49 => ENXIO,  // EUNATCH
        50 => ENXIO,  // ENOCSI
        51 => EPROTO, // EL2HLT
        52 => EPROTO, // EBADE
        53 => EINVAL, // EBADR: bad request descriptor
        54 => EPROTO, // EXFULL
        55 => ENXIO,  // ENOANO
        56 => EINVAL, // EBADRQC: bad request code
        57 => EINVAL, // EBADSLT: bad slot
        // 58: unused on Linux.
        59 => EFTYPE, // EBFONT: bad font file format
        60 => ENOSTR,
        61 => ENODATA,
        62 => ETIME,
        63 => ENOSR,
        64 => ENETDOWN, // ENONET: machine not on network
        65 => ENOSYS,   // ENOPKG: package not installed
        66 => EREMOTE,
        67 => ENOLINK,
        68 => EIO, // EADV: advertise error
        69 => EIO, // ESRMNT
        70 => EIO, // ECOMM: communication error on send
        71 => EPROTO,
        72 => EMULTIHOP,
        73 => EINVAL, // EDOTDOT: RFS-specific
        74 => EBADMSG,
        75 => EOVERFLOW,
        76 => EEXIST,    // ENOTUNIQ: name collision cousin
        77 => EBADF,     // EBADFD: fd in bad state
        78 => ENETRESET, // EREMCHG: peer changed
        79 => EACCES,    // ELIBACC: cannot access shared lib
        80 => EBADMACHO, // ELIBBAD: corrupted shared lib
        81 => EBADMACHO, // ELIBSCN: corrupted lib section
        82 => ENFILE,    // ELIBMAX: too many linked libs
        83 => ENOEXEC,   // ELIBEXEC: cannot exec shared lib
        84 => EILSEQ,
        85 => EINTR, // ERESTART: restart as interrupted
        86 => EPIPE, // ESTRPIPE: streams pipe error
        87 => EUSERS,
        88 => ENOTSOCK,
        89 => EDESTADDRREQ,
        90 => EMSGSIZE,
        91 => EPROTOTYPE,
        92 => ENOPROTOOPT,
        93 => EPROTONOSUPPORT,
        94 => ESOCKTNOSUPPORT,
        95 => EOPNOTSUPP,
        96 => EPFNOSUPPORT,
        97 => EAFNOSUPPORT,
        98 => EADDRINUSE,
        99 => EADDRNOTAVAIL,
        100 => ENETDOWN,
        101 => ENETUNREACH,
        102 => ENETRESET,
        103 => ECONNABORTED,
        104 => ECONNRESET,
        105 => ENOBUFS,
        106 => EISCONN,
        107 => ENOTCONN,
        108 => ESHUTDOWN,
        109 => ETOOMANYREFS,
        110 => ETIMEDOUT,
        111 => ECONNREFUSED,
        112 => EHOSTDOWN,
        113 => EHOSTUNREACH,
        114 => EALREADY,
        115 => EINPROGRESS,
        116 => ESTALE,
        117 => EIO,    // EUCLEAN: structure needs cleaning
        118 => EINVAL, // ENOTNAM: not a named file
        119 => ENOLCK, // ENAVAIL: no semaphores available
        120 => EISDIR, // EISNAM: is a named file
        121 => EIO,    // EREMOTEIO: remote I/O error
        122 => EDQUOT,
        123 => ENXIO,  // ENOMEDIUM: no medium found
        124 => EFTYPE, // EMEDIUMTYPE: wrong medium type
        125 => ECANCELED,
        126 => ENEEDAUTH, // ENOKEY: required key not available
        127 => EAUTH,     // EKEYEXPIRED
        128 => EAUTH,     // EKEYREVOKED
        129 => EAUTH,     // EKEYREJECTED
        130 => EOWNERDEAD,
        131 => ENOTRECOVERABLE,
        132 => EDEVERR, // ERFKILL: device-level error
        133 => EIO,     // EHWPOISON: hardware memory error
        _ => EIO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_darwin_errno_has_a_name() {
        for no in 1..=ELAST {
            assert!(name(no).is_some(), "errno {no} unnamed");
        }
        assert_eq!(name(0), None);
        assert_eq!(name(ELAST + 1), None);
    }

    #[test]
    fn names_cover_the_sdk_table() {
        // Spot-check both ends + the aliased pair: SDK defines
        // EOPNOTSUPP twice (ENOTSUP alias, then 102); the later
        // definition wins in C, so 102 owns the name.
        assert_eq!(name(EPERM), Some("EPERM"));
        assert_eq!(name(EAGAIN), Some("EAGAIN"));
        assert_eq!(name(ENOTSUP), Some("ENOTSUP"));
        assert_eq!(name(EOPNOTSUPP), Some("EOPNOTSUPP"));
        assert_eq!(name(ENOTCAPABLE), Some("ENOTCAPABLE"));
        assert_eq!(name(ELAST), Some("ENOTCAPABLE"));
    }

    #[test]
    fn shared_range_maps_identically() {
        // 1..=10 and 12..=34 share numbers on both OSes.
        for no in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 22, 32, 33, 34] {
            assert_eq!(from_linux(no), no);
        }
    }

    #[test]
    fn eagain_edeadlk_swap() {
        // The one trap in the low range: names cross, numbers don't.
        assert_eq!(from_linux(11), EAGAIN);
        assert_eq!(from_linux(35), EDEADLK);
        assert_eq!(name(from_linux(11)), Some("EAGAIN"));
        assert_eq!(name(from_linux(35)), Some("EDEADLK"));
    }

    #[test]
    fn socket_range_renumbers_by_name() {
        assert_eq!(name(from_linux(88)), Some("ENOTSOCK"));
        assert_eq!(name(from_linux(95)), Some("EOPNOTSUPP"));
        assert_eq!(name(from_linux(111)), Some("ECONNREFUSED"));
        assert_eq!(name(from_linux(36)), Some("ENAMETOOLONG"));
        assert_eq!(name(from_linux(40)), Some("ELOOP"));
    }

    #[test]
    fn linux_only_codes_get_cousins() {
        assert_eq!(name(from_linux(77)), Some("EBADF")); // EBADFD
        assert_eq!(name(from_linux(85)), Some("EINTR")); // ERESTART
        assert_eq!(name(from_linux(126)), Some("ENEEDAUTH")); // ENOKEY
        assert_eq!(name(from_linux(133)), Some("EIO")); // EHWPOISON
    }

    #[test]
    fn unknown_maps_to_eio_never_passthrough() {
        assert_eq!(from_linux(0), EIO);
        assert_eq!(from_linux(-1), EIO);
        assert_eq!(from_linux(41), EIO); // unused on Linux
        assert_eq!(from_linux(1000), EIO);
    }
}
