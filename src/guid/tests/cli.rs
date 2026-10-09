use testutil::Tool;

const TOOL: Tool = Tool::new(env!("CARGO_BIN_EXE_guid"));

/// Checks the shape of a v4 GUID: 8-4-4-4-12 hex groups, version nibble 4,
/// and an RFC 4122 variant nibble (8/9/a/b).
fn is_valid_v4_guid(s: &str, uppercase: bool) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        let expect_hyphen = matches!(i, 8 | 13 | 18 | 23);
        if expect_hyphen {
            if b != b'-' {
                return false;
            }
            continue;
        }
        let is_hex_digit = if uppercase {
            b.is_ascii_digit() || (b'A'..=b'F').contains(&b)
        } else {
            b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
        };
        if !is_hex_digit {
            return false;
        }
    }
    let version = bytes[14];
    let variant = bytes[19].to_ascii_lowercase();
    version == b'4' && matches!(variant, b'8' | b'9' | b'a' | b'b')
}

#[test]
fn default_prints_one_valid_v4_guid() {
    let out = TOOL.stdout(&[]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(is_valid_v4_guid(lines[0], false), "not a v4 guid: {}", lines[0]);
}

#[test]
fn count_prints_that_many_guids() {
    let out = TOOL.stdout(&["-c", "5"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 5);
    for line in lines {
        assert!(is_valid_v4_guid(line, false));
    }
}

#[test]
fn empty_prints_the_nil_guid() {
    let out = TOOL.stdout(&["-e"]);
    assert_eq!(out.trim(), "00000000-0000-0000-0000-000000000000");
}

#[test]
fn from_integer_encodes_the_value_into_the_guid() {
    // 0xDEADBEEF
    let out = TOOL.stdout(&["-i", "3735928559"]);
    assert_eq!(
        out.trim(),
        "00000000-0000-0000-0000-0000deadbeef"
    );
}

#[test]
fn uppercase_flag_uppercases_the_output() {
    let out = TOOL.stdout(&["-i", "3735928559", "-u"]);
    assert_eq!(out.trim(), "00000000-0000-0000-0000-0000DEADBEEF");
}

#[test]
fn lowercase_is_the_default() {
    let out = TOOL.stdout(&["-i", "3735928559", "-l"]);
    assert_eq!(out.trim(), "00000000-0000-0000-0000-0000deadbeef");
}

#[test]
fn from_integer_and_empty_conflict() {
    let out = TOOL.run(&["-i", "1", "-e"]);
    assert!(!out.status.success());
}

#[test]
fn uppercase_and_lowercase_conflict() {
    let out = TOOL.run(&["-u", "-l"]);
    assert!(!out.status.success());
}
