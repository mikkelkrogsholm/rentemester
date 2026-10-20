//! Decode only the encoding policy declared by the selected import profile.
use super::{Error, fail, profile_name};
const LIMIT: usize = 25 * 1024 * 1024;
// WHATWG windows-1252 index, including the five C1 identity mappings used by
// the source TextDecoder (0x81, 0x8d, 0x8f, 0x90 and 0x9d).
const C1: [char; 32] = [
    '€', '\u{0081}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{008d}', 'Ž',
    '\u{008f}', '\u{0090}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{009d}',
    'ž', 'Ÿ',
];
pub fn decode_bytes(bytes: &[u8], profile: Option<&str>) -> Result<String, Error> {
    let profile = profile_name(profile)?;
    if bytes.len() > LIMIT {
        return Err(fail("bank CSV exceeds 25 MiB"));
    }
    let utf8 = std::str::from_utf8(bytes);
    let content = if profile.is_some()
        && utf8
            .as_ref()
            .ok()
            .is_none_or(|value| value.contains('\u{fffd}'))
    {
        bytes
            .iter()
            .map(|byte| match byte {
                128..=159 => C1[usize::from(*byte) - 128],
                _ => char::from(*byte),
            })
            .collect()
    } else {
        let text = utf8.map_err(|_|fail("generic bank import requires valid UTF-8; select a declared profile for legacy encoding"))?;
        text.strip_prefix('\u{feff}').unwrap_or(text).to_owned()
    };
    if content.len() > LIMIT {
        return Err(fail("decoded bank CSV exceeds 25 MiB"));
    }
    Ok(content)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_decoder_preserves_danish_and_whatwg_windows1252_characters() {
        let bytes =
            b"Dato;Tekst;Bel\xf8b\r\n01.04.2026;\xc6ble \xd8re \xc5 og \xe6\xf8\xe5 \x80;100\r\n";
        let decoded = decode_bytes(bytes, Some(" Danske-Bank ")).unwrap();
        assert!(decoded.contains("Æble Øre Å og æøå €"));
        assert!(decoded.contains("\r\n"));
        assert_eq!(
            decode_bytes(&[0x81, 0x8d, 0x8f, 0x90, 0x9d], Some("danske-bank")).unwrap(),
            "\u{0081}\u{008d}\u{008f}\u{0090}\u{009d}"
        );
        let literal_replacement = "�";
        assert_eq!(
            decode_bytes(literal_replacement.as_bytes(), None).unwrap(),
            literal_replacement
        );
        assert_eq!(
            decode_bytes(literal_replacement.as_bytes(), Some("danske-bank")).unwrap(),
            "ï¿½"
        );
    }
    #[test]
    fn utf8_bom_and_generic_admission_remain_explicit_and_bounded() {
        let source = "\u{feff}date;text;amount\r\n2026-04-01;Æble €;100\r\n";
        assert_eq!(
            decode_bytes(source.as_bytes(), None).unwrap(),
            source.trim_start_matches('\u{feff}')
        );
        assert_eq!(
            decode_bytes(source.as_bytes(), Some("danske-bank")).unwrap(),
            source.trim_start_matches('\u{feff}')
        );
        assert!(decode_bytes(b"invalid\xff", None).is_err());
        assert!(decode_bytes(b"text", Some("unknown")).is_err());
        assert!(decode_bytes(&vec![b'a'; LIMIT + 1], Some("danske-bank")).is_err());
        assert!(decode_bytes(&vec![0x80; LIMIT / 2 + 1], Some("danske-bank")).is_err());
    }
}
