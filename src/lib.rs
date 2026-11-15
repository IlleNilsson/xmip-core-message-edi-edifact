#![forbid(unsafe_code)]

//! UN/EDIFACT: an interchange as its segments, one part each, named by the
//! tag — `UNB`, `UNH`, `BGM`, `UNT`, `UNZ` — in the order they lie. The
//! delimiters are the service string advice's when the interchange opens
//! with `UNA`, and the default set `:+.? '` when it opens with `UNB`
//! directly; the `UNA` itself is the first part. The announced type is the
//! message type of the first `UNH`, the first component of S009 — `ORDERS`,
//! `INVOIC`, `DESADV` — when there is a `UNH` (ADR-0047).
//!
//! The walk is the Foundation's `message::segment`: this shape brings the
//! delimiters and reads one segment. A contract checks everything else.
//! The `UNA` is read here once ([`advice`]), and the EDIFACT contract reads
//! an interchange's head by it too (ADR-0044, amendment 2026-11-15).

use message::segment::{self, Delimiters};
use message::{Shape, ShapeError, Shaped};
use stream::Stream;

/// The UN/EDIFACT shape.
#[derive(Clone, Copy, Debug, Default)]
pub struct Edifact;

/// The delimiters an interchange has when it announces none: `:+.? '`.
pub const DEFAULT: Delimiters = Delimiters::new(b'\'', b'+', b':').with_release(b'?');

/// The length of a `UNA`: its tag and the six service characters it
/// advises.
pub const UNA_LENGTH: usize = 9;

/// What the head of an interchange advises: the delimiters in force, and
/// the `UNA` that advised them where there is one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Advice<'a> {
    pub delimiters: Delimiters,
    /// The `UNA` as it is written, its [`UNA_LENGTH`] bytes; none where the
    /// interchange opens without one.
    pub una: Option<&'a [u8]>,
}

/// The service characters the `UNA` at the head of `bytes` advises —
/// component, element, decimal mark, release, reserved, terminator — else
/// the default set. A space advised as the release character is none, as
/// ISO 9735 syntax 3 says (*if not used, insert space character*). The
/// walk reads bytes, so every service character is ASCII.
///
/// # Errors
/// A `UNA` shorter than its nine characters, or one with a service
/// character outside ASCII.
pub fn advice(bytes: &[u8]) -> Result<Advice<'_>, ShapeError> {
    if !bytes.starts_with(b"UNA") {
        return Ok(Advice {
            delimiters: DEFAULT,
            una: None,
        });
    }
    let Some(una) = bytes.get(..UNA_LENGTH) else {
        return Err(
            ShapeError::new("edi-edifact", "a UNA shorter than its nine characters")
                .at(bytes.len()),
        );
    };
    if let Some(at) = una.iter().position(|byte| !byte.is_ascii()) {
        let reason = "a UNA with a service character outside ASCII";
        return Err(ShapeError::new("edi-edifact", reason).at(at));
    }
    let announced = Delimiters::new(una[8], una[4], una[3]);
    let delimiters = match una[6] {
        b' ' => announced,
        release => announced.with_release(release),
    };
    Ok(Advice {
        delimiters,
        una: Some(una),
    })
}

impl Shape for Edifact {
    fn technology(&self) -> &'static str {
        "edi-edifact"
    }

    fn media_types(&self) -> &'static [&'static str] {
        &["application/edifact"]
    }

    fn recognizes(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(b"UNA") || bytes.starts_with(b"UNB")
    }

    fn shape(&self, stream: &Stream) -> Result<Shaped, ShapeError> {
        let bytes = message::loaded(self.technology(), stream)?;
        let delimiters = advice(bytes)?.delimiters;
        let segments = segment::segments(bytes, &delimiters)
            .map_err(|stop| ShapeError::refused("edi-edifact", stop))?;
        let message_type = segment::first(&segments, "UNH")
            .and_then(|unh| unh.component(1, 0, &delimiters))
            .filter(|kind| !kind.is_empty())
            .map(|kind| String::from_utf8_lossy_owned(delimiters.unreleased(kind)));
        let media = stream.media_type().unwrap_or("application/edifact");
        Ok(Shaped {
            parts: segment::parts(&segments, media),
            message_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xcore::StreamId;

    const ORDERS: &[u8] = b"UNA:+.? 'UNB+UNOA:2+SENDER+RECEIVER+240101:1200+1'\r\n\
UNH+1+ORDERS:D:96A:UN'\r\nBGM+220+PO?+1+9'\r\nFTX+AAA+++Bob?'s co'\r\n\
UNT+4+1'\r\nUNZ+1+1'\r\n";

    fn stream(bytes: &[u8], media: Option<&str>) -> Stream {
        Stream::new(StreamId::new(1), bytes.to_vec(), media.map(str::to_string))
    }

    #[test]
    fn an_interchange_is_its_segments_named_by_tag_and_the_unh_message_type_is_announced() {
        let shaped = Edifact.shape(&stream(ORDERS, None)).expect("well-formed");
        let names: Vec<&str> = shaped
            .parts
            .iter()
            .filter_map(|p| p.name.as_deref())
            .collect();
        assert_eq!(names, ["UNA", "UNB", "UNH", "BGM", "FTX", "UNT", "UNZ"]);
        assert_eq!(shaped.parts[0].bytes, b"UNA:+.? ");
        assert_eq!(shaped.parts[3].bytes, b"BGM+220+PO?+1+9");
        assert_eq!(shaped.parts[4].bytes, b"FTX+AAA+++Bob?'s co");
        assert_eq!(
            shaped.parts[6].media_type.as_deref(),
            Some("application/edifact")
        );
        assert_eq!(shaped.message_type.as_deref(), Some("ORDERS"));

        let typed = Edifact
            .shape(&stream(
                ORDERS,
                Some("application/edifact; charset=iso-8859-1"),
            ))
            .expect("well-formed");
        assert_eq!(
            typed.parts[1].media_type.as_deref(),
            Some("application/edifact; charset=iso-8859-1")
        );
    }

    #[test]
    fn a_una_announces_other_delimiters_and_without_una_or_unh_the_defaults_and_no_type_apply() {
        let other = b"UNA|^.! ~UNB^UNOA|2^S^R^240101|1200^1~UNH^1^INVOIC|D|96A|UN~UNZ^1^1~";
        let shaped = Edifact.shape(&stream(other, None)).expect("well-formed");
        assert_eq!(shaped.parts.len(), 4);
        assert_eq!(shaped.parts[2].name.as_deref(), Some("UNH"));
        assert_eq!(shaped.message_type.as_deref(), Some("INVOIC"));
        let advised = advice(other).expect("advised");
        assert_eq!(advised.delimiters.release, Some(b'!'));
        assert_eq!(advised.una, Some(&other[..UNA_LENGTH]));

        let plain = Edifact
            .shape(&stream(b"UNB+UNOA:2+S+R+240101:1200+1'UNZ+0+1'", None))
            .expect("defaults");
        assert_eq!(plain.parts.len(), 2);
        assert_eq!(plain.message_type, None);
        let default = Advice {
            delimiters: DEFAULT,
            una: None,
        };
        assert_eq!(advice(b"UNB+"), Ok(default));
    }

    /// A space in UNA4 is no release character (ISO 9735 syntax 3: *if not
    /// used, insert space character*): `?` is then a character like any.
    #[test]
    fn a_space_advised_as_the_release_character_is_none() {
        let unreleased = b"UNA:+.  'UNB+UNOC:3+S+R'FTX+AAI+++Why? Because'UNZ+0+R'";
        assert_eq!(advice(unreleased).map(|a| a.delimiters.release), Ok(None));
        let shaped = Edifact
            .shape(&stream(unreleased, None))
            .expect("well-formed");
        assert_eq!(shaped.parts[2].bytes, b"FTX+AAI+++Why? Because");
        assert_eq!(shaped.parts.len(), 4);
    }

    #[test]
    fn a_short_una_or_a_cut_segment_or_an_empty_stream_is_refused_where_it_fails() {
        let short = Edifact.shape(&stream(b"UNA:+.", None)).expect_err("short");
        assert_eq!(short.offset, Some(6));
        assert_eq!(
            short.to_string(),
            "edi-edifact: a UNA shorter than its nine characters at byte 6"
        );
        let wide = Edifact
            .shape(&stream("UNA:+.? \u{e9}UNB+1\u{e9}".as_bytes(), None))
            .expect_err("outside ASCII");
        assert_eq!(wide.offset, Some(8));
        assert_eq!(wide.reason, "a UNA with a service character outside ASCII");

        let cut = Edifact
            .shape(&stream(b"UNB+UNOA:2+S+R'UNH+1+ORDERS:D", None))
            .expect_err("no terminator");
        assert_eq!(cut.offset, Some(15));
        assert_eq!(cut.reason, "a segment without its terminator");

        let empty = Edifact.shape(&stream(b"\r\n", None)).expect_err("nothing");
        assert_eq!(empty.reason, "no segment at all");
        assert_eq!(empty.technology, "edi-edifact");
    }

    #[test]
    fn the_shape_claims_edifact_and_recognizes_una_or_unb_at_the_head() {
        assert_eq!(Edifact.technology(), "edi-edifact");
        assert_eq!(Edifact.media_types(), &["application/edifact"]);
        assert!(Edifact.recognizes(b"UNA:+.? 'UNB"));
        assert!(Edifact.recognizes(b"UNB+UNOA:2"));
        assert!(!Edifact.recognizes(b"ISA*00*"));
        assert!(!Edifact.recognizes(b""));

        let shapes: [&dyn Shape; 1] = [&Edifact];
        let by_media =
            message::choose(&shapes, &stream(b"x", Some("Application/EDIFACT"))).expect("held");
        assert_eq!(by_media.map(Shape::technology), Some("edi-edifact"));
        let by_look = message::choose(&shapes, &stream(ORDERS, None)).expect("held");
        assert_eq!(by_look.map(Shape::technology), Some("edi-edifact"));
        assert!(
            message::choose(&shapes, &stream(b"MSH|", None))
                .expect("held")
                .is_none()
        );
    }
}
