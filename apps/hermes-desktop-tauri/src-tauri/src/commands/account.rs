use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AccountSnapshot {
    pub signed_in: bool,
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub tier: String,
}

#[tauri::command]
pub async fn sign_in_oauth(provider: String) -> Result<AccountSnapshot, String> {
    Err(format!(
        "not-supported: Terra Cloud OAuth ({provider}) pending — configure HERMES_CLOUD_AUTH_URL"
    ))
}

#[tauri::command]
pub async fn sign_in_email(email: String, _otp: String) -> Result<AccountSnapshot, String> {
    Err(format!(
        "not-supported: email OTP sign-in pending for {email}"
    ))
}

#[tauri::command]
pub async fn sign_out() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn get_account() -> Result<AccountSnapshot, String> {
    Ok(AccountSnapshot {
        signed_in: false,
        user_id: None,
        email: None,
        tier: "free".into(),
    })
}

#[tauri::command]
pub async fn refresh_account_token() -> Result<AccountSnapshot, String> {
    get_account().await
}
