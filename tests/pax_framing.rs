use std::io::{Cursor, Read};
use tar::{Archive, Builder, EntryType, Header, PaxExtensions};

fn record(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut len = key.len() + value.len() + 4;
    loop {
        let next = len.to_string().len() + key.len() + value.len() + 3;
        if next == len {
            break;
        }
        len = next;
    }
    let mut data = format!("{} ", len).into_bytes();
    data.extend_from_slice(key);
    data.push(b'=');
    data.extend_from_slice(value);
    data.push(b'\n');
    assert_eq!(data.len(), len);
    data
}

#[test]
fn values_are_length_delimited_bytes() {
    for value in [b"hello\nworld".as_slice(), b"\n\n", b"\0\xff\n=", b""] {
        let mut data = record(b"SCHILY.xattr.user.note", value);
        data.extend(record(b"SCHILY.xattr.user.overlay.opaque", b"y"));
        let mut exts = PaxExtensions::new(&data);
        let first = exts.next().unwrap().unwrap();
        assert_eq!(first.key_bytes(), b"SCHILY.xattr.user.note");
        assert_eq!(first.value_bytes(), value);
        let second = exts.next().unwrap().unwrap();
        assert_eq!(second.key_bytes(), b"SCHILY.xattr.user.overlay.opaque");
        assert_eq!(second.value_bytes(), b"y");
        assert!(exts.next().is_none());
    }
}

#[test]
fn length_digit_boundaries() {
    for size in [0, 1, 4, 5, 6, 93, 94, 95, 993, 994, 995] {
        let value = vec![b'\n'; size];
        let data = record(b"k", &value);
        assert_eq!(
            PaxExtensions::new(&data)
                .next()
                .unwrap()
                .unwrap()
                .value_bytes(),
            value
        );
    }
}

#[test]
fn malformed_records_fail_without_resynchronizing() {
    for data in [
        b"0 k=v\n".as_slice(),
        b"1 k=v\n",
        b"999 k=v\n",
        b"999999999999999999999999999999999999 k=v\n",
        b"+6 k=\n",
        b"-6 k=\n",
        b"x k=v\n",
        b" k=v\n",
        b"5 k=v\n",
        b"7 k=v!",
        b"7 k=v",
        b"6 =ab\n",
        b"6 kab\n",
        b"7 k=v\n",
        b"\n",
    ] {
        let mut exts = PaxExtensions::new(data);
        assert!(exts.next().unwrap().is_err(), "accepted {:?}", data);
        assert!(exts.next().is_none());
    }
    assert!(PaxExtensions::new(b"").next().is_none());
    let valid = record(b"k", b"v");
    for end in 1..valid.len() {
        assert!(PaxExtensions::new(&valid[..end]).next().unwrap().is_err());
    }
}

#[test]
fn archive_path_and_size_follow_multiline_xattr() {
    let mut builder = Builder::new(Vec::new());
    builder
        .append_pax_extensions([
            ("SCHILY.xattr.user.note", b"hello\nworld".as_slice()),
            ("path", b"directory/line\nbreak".as_slice()),
            ("size", b"4".as_slice()),
        ])
        .unwrap();
    let mut header = Header::new_ustar();
    header.set_path("placeholder").unwrap();
    header.set_size(1);
    header.set_mode(0o644);
    header.set_entry_type(EntryType::Regular);
    header.set_cksum();
    let mut bytes = builder.into_inner().unwrap();
    // Replace the end markers with a header whose PAX size overrides its size.
    bytes.truncate(1024);
    bytes.extend_from_slice(header.as_bytes());
    bytes.extend_from_slice(b"data");
    bytes.resize(2048, 0);
    bytes.resize(3072, 0);
    let mut archive = Archive::new(Cursor::new(bytes));
    let mut entries = archive.entries().unwrap();
    let mut entry = entries.next().unwrap().unwrap();
    assert_eq!(entry.path_bytes(), b"directory/line\nbreak".as_slice());
    let mut content = String::new();
    entry.read_to_string(&mut content).unwrap();
    assert_eq!(content, "data");
    assert!(entries.next().is_none());
}
