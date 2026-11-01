//! The guard: what reaches the engine when something goes wrong inside.

use super::*;

fn last_error() -> String {
    // SAFETY: a buffer of the length the first call gave.
    unsafe {
        let n = omni_last_error(std::ptr::null_mut(), 0);
        let mut buf = vec![0u8; n];
        omni_last_error(buf.as_mut_ptr(), n);
        String::from_utf8(buf).unwrap()
    }
}

#[test]
fn a_panic_becomes_a_status_and_a_message() {
    // Quiet: the default hook would print the panic to stderr.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let code = guard(|| panic!("boom {}", 7));
    std::panic::set_hook(hook);
    assert_eq!(code, OMNI_ERR_PANIC);
    assert!(last_error().contains("boom 7"), "{}", last_error());
}

#[test]
fn an_error_keeps_its_code() {
    assert_eq!(
        guard(|| Err((OMNI_ERR_LAYER, "nope".into()))),
        OMNI_ERR_LAYER
    );
    assert_eq!(last_error(), "nope");
    assert_eq!(guard(|| Ok(())), OMNI_OK);
}

#[test]
fn errors_are_per_thread() {
    guard(|| Err((OMNI_ERR_ARGUMENT, "here".into())));
    std::thread::spawn(|| {
        guard(|| Err((OMNI_ERR_ARGUMENT, "there".into())));
        assert_eq!(last_error(), "there");
    })
    .join()
    .unwrap();
    assert_eq!(last_error(), "here");
}

#[test]
fn params_are_a_json_object() {
    assert_eq!(params("").unwrap(), []);
    assert_eq!(
        params(r#"{"a": 2, "b": "x", "c": true}"#).unwrap(),
        [
            ("a".into(), "2".into()),
            ("b".into(), "x".into()),
            ("c".into(), "true".into())
        ]
    );
    assert_eq!(params("[1]").unwrap_err().0, OMNI_ERR_ARGUMENT);
}
