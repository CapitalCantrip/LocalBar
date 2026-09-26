struct Segment<'a> {
    text: &'a str,
    separator: char,
}

fn make_segment(text: &str) -> Segment {
    Segment { text, separator: '/' }
}
