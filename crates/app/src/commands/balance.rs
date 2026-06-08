//! `get_user_balance` command — fetches the authenticated user's
//! provider wallet balances.
//!
//! Defaults to provider `"deepseek"` when input is absent. Other
//! providers are forward-compatible stubs returning `Ok(None)` so the
//! frontend can degrade gracefully instead of erroring out.

use deepseek_client::deepseek::pick_primary_balance;

use crate::dto::{BalanceInfoDto, GetUserBalanceInput, UserBalanceDto};
use crate::AppState;

/// Hit the active provider's balance endpoint and return a UI-friendly
/// shape. All failure modes (transport error, non-2xx, deserialize bug,
/// unsupported provider) collapse to `Ok(None)` so the balance UI cell
/// can hide instead of breaking the chat path. See Reasonix's
/// `getBalance` pattern (`src/client.ts:251-264`) — same philosophy.
#[tauri::command]
pub async fn get_user_balance(
    state: tauri::State<'_, AppState>,
    input: Option<GetUserBalanceInput>,
) -> Result<Option<UserBalanceDto>, String> {
    let provider = input
        .and_then(|i| i.provider_id)
        .unwrap_or_else(|| "deepseek".into());

    match provider.as_str() {
        "deepseek" => match state.engine.client().get_user_balance().await {
            Ok(Some(b)) => {
                let primary_currency = pick_primary_balance(&b.balance_infos)
                    .map(|p| p.currency.clone())
                    .unwrap_or_default();
                Ok(Some(UserBalanceDto {
                    is_available: b.is_available,
                    primary_currency,
                    balance_infos: b.balance_infos.into_iter().map(Into::into).collect(),
                }))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                tracing::warn!(error = %e, "get_user_balance unexpected error; degrading to None");
                Ok(None)
            }
        },
        // Forward-compat: future providers (openai/anthropic/openrouter)
        // either land here with a real implementation or stay None until
        // the integration ships.
        _ => Ok(None),
    }
}

impl From<deepseek_client::deepseek::BalanceInfo> for BalanceInfoDto {
    fn from(b: deepseek_client::deepseek::BalanceInfo) -> Self {
        Self {
            currency: b.currency,
            total: b.total_balance.parse().unwrap_or(0.0),
            granted: b.granted_balance.and_then(|s| s.parse().ok()),
            topped_up: b.topped_up_balance.and_then(|s| s.parse().ok()),
        }
    }
}
