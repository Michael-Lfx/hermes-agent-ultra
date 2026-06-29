use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PaymentOrder {
    pub order_id: String,
    pub status: String,
    pub provider: String,
}

#[tauri::command]
pub async fn create_alipay_order(product_id: String) -> Result<PaymentOrder, String> {
    Err(format!(
        "not-supported: Alipay order for {product_id} — handled by Terra Cloud billing"
    ))
}

#[tauri::command]
pub async fn create_wechat_order(product_id: String) -> Result<PaymentOrder, String> {
    Err(format!(
        "not-supported: WeChat Pay order for {product_id} — handled by Terra Cloud billing"
    ))
}

#[tauri::command]
pub async fn query_payment_status(order_id: String) -> Result<PaymentOrder, String> {
    Ok(PaymentOrder {
        order_id,
        status: "pending".into(),
        provider: "stub".into(),
    })
}
