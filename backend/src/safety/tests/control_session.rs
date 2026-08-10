use crate::safety::{ControlSession, ControlSessionError, CONTROL_SESSION_HEADER};

#[test]
fn generated_tokens_have_256_bits_of_uuid_material_and_are_distinct() {
    let first = ControlSession::generate();
    let second = ControlSession::generate();

    assert_eq!(first.token().len(), 64);
    assert_eq!(second.token().len(), 64);
    assert_ne!(first.token(), second.token());
    assert!(first
        .token()
        .chars()
        .all(|character| character.is_ascii_hexdigit()));
}

#[test]
fn only_the_exact_control_session_token_is_accepted() {
    let token = "a".repeat(64);
    let session = ControlSession::new(token.clone()).unwrap();

    assert_eq!(session.verify(Some(&token)), Ok(()));
    assert_eq!(session.verify(None), Err(ControlSessionError::Missing));
    assert_eq!(
        session.verify(Some(&"A".repeat(64))),
        Err(ControlSessionError::Invalid)
    );
}

#[test]
fn short_or_blank_tokens_are_rejected() {
    assert_eq!(
        ControlSession::new("short"),
        Err(ControlSessionError::TooShort)
    );
    assert_eq!(
        ControlSession::new(" ".repeat(64)),
        Err(ControlSessionError::TooShort)
    );
}

#[test]
fn header_name_is_stable() {
    assert_eq!(CONTROL_SESSION_HEADER, "x-yilian-control-session");
}
