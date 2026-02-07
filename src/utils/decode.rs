use chardetng::EncodingDetector;
use std::borrow::Cow;

pub fn decode_text(input: &[u8]) -> Cow<'_, str> {
    if input.is_empty() {
        return Cow::Borrowed("");
    }
    let mut detector = EncodingDetector::new();
    detector.feed(input, true);
    let encoding = detector.guess(None, true);
    let (cow, _, _) = encoding.decode(input);
    cow
}
