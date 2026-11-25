use super::*;
fn absent_tree_child(code: i32) -> bool {
    code == 0
}

pub(super) fn optional_pattern<T>(
    result: windows::core::Result<T>,
) -> Result<Option<T>, SoftwareError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e)
            if matches!(
                e.code().0 as u32,
                0 | 0x80004002 | 0x80004003 | UIA_E_NOTSUPPORTED
            ) =>
        {
            Ok(None)
        }
        Err(e) => Err(error(
            "uia_provider_failed",
            format!("Pattern request failed: {e}"),
        )),
    }
}
pub(super) fn optional_child(
    result: windows::core::Result<IUIAutomationElement>,
) -> Result<Option<IUIAutomationElement>, SoftwareError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) if absent_tree_child(e.code().0) => Ok(None),
        Err(e) => Err(error(
            "uia_provider_failed",
            format!("Tree traversal failed: {e}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nullable_tree_result_is_not_a_provider_failure() {
        assert!(absent_tree_child(0));
        for failure in [
            0x80004003u32,
            0x80070005u32,
            UIA_E_TIMEOUT,
            UIA_E_ELEMENTNOTAVAILABLE,
            UIA_E_NOTSUPPORTED,
        ] {
            assert!(!absent_tree_child(failure as i32));
        }
        let denied = optional_child(Err(windows::core::Error::from_hresult(
            windows::core::HRESULT(0x80070005u32 as i32),
        )))
        .err()
        .unwrap();
        assert_eq!(denied.code, "uia_provider_failed");
    }
    #[test]
    fn unsupported_null_pattern_is_absent_but_denied_pattern_is_an_error() {
        let request = |code| {
            optional_pattern::<()>(Err(windows::core::Error::from_hresult(
                windows::core::HRESULT(code),
            )))
        };
        assert!(matches!(request(0), Ok(None)));
        assert_eq!(
            request(0x80070005u32 as i32).unwrap_err().code,
            "uia_provider_failed"
        );
    }
    #[test]
    fn verifies_a_delayed_provider_update_without_repeating_the_action() {
        let mut polls = 0;
        verify_readback(
            || {
                polls += 1;
                Ok(polls == 2)
            },
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(polls, 2);
    }
    #[test]
    fn expired_uia_budget_refuses_com_creation() {
        let budget = Budget {
            deadline: Instant::now() - Duration::from_millis(1),
        };
        let error = client(&budget)
            .err()
            .expect("Expired work must be rejected before COM activation");
        assert_eq!(error.code, "uia_timeout");
        assert!(error.message.contains("com_create_uia8"));
    }
}
