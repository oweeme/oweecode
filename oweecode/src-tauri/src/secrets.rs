// Stores passwords in the OS keychain (Secret Service / Keychain / Credential
// Manager) instead of plain text in the webview's localStorage.

const SERVICE: &str = "oweecode";

fn entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secret_set(key: String, value: String) -> Result<(), String> {
    entry(&key)?.set_password(&value).map_err(|e| e.to_string())
}

/// Returns None when nothing is stored for `key`.
#[tauri::command]
pub fn secret_get(key: String) -> Result<Option<String>, String> {
    match entry(&key)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn secret_delete(key: String) -> Result<(), String> {
    match entry(&key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn keychain_roundtrip() {
        let k = "test:oweecode-roundtrip".to_string();
        match super::secret_set(k.clone(), "abc123".into()) {
            Ok(()) => {
                assert_eq!(
                    super::secret_get(k.clone()).unwrap().as_deref(),
                    Some("abc123")
                );
                super::secret_delete(k.clone()).unwrap();
                assert_eq!(super::secret_get(k).unwrap(), None);
            }
            Err(e) => println!("KEYCHAIN_UNAVAILABLE: {e}"),
        }
    }
}
