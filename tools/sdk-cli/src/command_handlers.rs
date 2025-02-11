use crate::persist::CliPersistence;
use crate::Commands;
use anyhow::Error;
use anyhow::{anyhow, Result};
use breez_sdk_core::Config;
use breez_sdk_core::InputType::{LnUrlAuth, LnUrlWithdraw};
use breez_sdk_core::{
    parse, BreezEvent, BreezServices, EventListener, GreenlightCredentials, InputType::LnUrlPay,
    PaymentTypeFilter,
};
use once_cell::sync::{Lazy, OnceCell};
use rustyline::Editor;
use std::sync::Arc;

static BREEZ_SERVICES: OnceCell<Arc<BreezServices>> = OnceCell::new();
static RT: Lazy<tokio::runtime::Runtime> = Lazy::new(|| tokio::runtime::Runtime::new().unwrap());

fn sdk() -> Result<Arc<BreezServices>> {
    BREEZ_SERVICES
        .get()
        .ok_or("Breez Services not initialized")
        .map_err(|err| anyhow!(err))
        .cloned()
}

fn rt() -> &'static tokio::runtime::Runtime {
    &RT
}

struct CliEventListener {}
impl EventListener for CliEventListener {
    fn on_event(&self, e: BreezEvent) {
        info!("Received Breez event: {:?}", e);
    }
}

async fn init_sdk(config: Config, seed: &[u8], creds: &GreenlightCredentials) -> Result<()> {
    let service = BreezServices::init_services(
        config,
        seed.to_vec(),
        creds.clone(),
        Box::new(CliEventListener {}),
    )
    .await?;

    BREEZ_SERVICES
        .set(service)
        .map_err(|_| anyhow!("Failed to set Breez Service"))?;

    BreezServices::start(rt(), &sdk()?).await
}

pub(crate) async fn handle_command(
    rl: &mut Editor<()>,
    persistence: &CliPersistence,
    command: Commands,
) -> Result<String, Error> {
    match command {
        Commands::SetAPIKey { key } => {
            let mut config = persistence.get_or_create_config()?;
            config.api_key = Some(key);
            persistence.save_config(config)?;
            Ok("API key was set".to_string())
        }
        Commands::SetEnv { env } => {
            let mut config = persistence.get_or_create_config()?;
            config.env = env.clone();
            persistence.save_config(config)?;
            Ok(format!("Environment was set to {:?}", env))
        }
        Commands::RegisterNode {} => {
            let config = persistence
                .get_or_create_config()?
                .to_sdk_config(&persistence.data_dir);
            let creds = BreezServices::register_node(
                config.network,
                persistence.get_or_create_seed().to_vec(),
            )
            .await?;

            init_sdk(config, &persistence.get_or_create_seed(), &creds).await?;
            persistence.save_credentials(creds)?;
            Ok("Node was registered succesfully".to_string())
        }
        Commands::RecoverNode {} => {
            let config = persistence
                .get_or_create_config()?
                .to_sdk_config(&persistence.data_dir);
            let creds = BreezServices::recover_node(
                config.network,
                persistence.get_or_create_seed().to_vec(),
            )
            .await?;

            init_sdk(config, &persistence.get_or_create_seed(), &creds).await?;
            persistence.save_credentials(creds)?;
            Ok("Node was recovered succesfully".to_string())
        }
        Commands::Init {} => match persistence.credentials() {
            Some(creds) => {
                let config = persistence
                    .get_or_create_config()?
                    .to_sdk_config(&persistence.data_dir);
                init_sdk(config, &persistence.get_or_create_seed(), &creds).await?;
                Ok("Node was initialized succesfully".to_string())
            }
            None => Err(anyhow!("Credentials not found")),
        },
        Commands::Sync {} => {
            sdk()?.sync().await?;
            Ok("Sync finished succesfully".to_string())
        }
        Commands::ReceivePayment {
            amount,
            description,
        } => {
            let res = sdk()?.receive_payment(amount, description).await?;
            serde_json::to_string_pretty(&res).map_err(|e| e.into())
        }
        Commands::SendPayment { bolt11, amount } => {
            let payment = sdk()?.send_payment(bolt11, amount).await?;
            serde_json::to_string_pretty(&payment).map_err(|e| e.into())
        }
        Commands::SendSpontaneousPayment { node_id, amount } => {
            let payment = sdk()?.send_spontaneous_payment(node_id, amount).await?;
            serde_json::to_string_pretty(&payment).map_err(|e| e.into())
        }
        Commands::ListPayments {} => {
            let payments = sdk()?
                .list_payments(PaymentTypeFilter::All, None, None)
                .await?;
            serde_json::to_string_pretty(&payments).map_err(|e| e.into())
        }
        Commands::Sweep {
            to_address,
            sat_per_byte,
        } => {
            sdk()?.sweep(to_address, sat_per_byte).await?;
            Ok("Onchain funds were swept succesfully".to_string())
        }
        Commands::ListLsps {} => {
            let lsps = sdk()?.list_lsps().await?;
            serde_json::to_string_pretty(&lsps).map_err(|e| e.into())
        }
        Commands::ConnectLSP { lsp_id } => {
            sdk()?.connect_lsp(lsp_id).await?;
            Ok("LSP connected succesfully".to_string())
        }
        Commands::NodeInfo {} => {
            serde_json::to_string_pretty(&sdk()?.node_info()?).map_err(|e| e.into())
        }
        Commands::ListFiat {} => {
            serde_json::to_string_pretty(&sdk()?.list_fiat_currencies().await?)
                .map_err(|e| e.into())
        }
        Commands::FetchFiatRates {} => {
            serde_json::to_string_pretty(&sdk()?.fetch_fiat_rates().await?).map_err(|e| e.into())
        }
        Commands::CloseLSPChannels {} => {
            sdk()?.close_lsp_channels().await?;
            Ok("LSP channels were closed succesfully".to_string())
        }
        Commands::StopNode {} => {
            sdk()?.stop().await?;
            Ok("Node was stopped succesfully".to_string())
        }
        Commands::RecommendedFees {} => {
            serde_json::to_string_pretty(&sdk()?.recommended_fees().await?).map_err(|e| e.into())
        }
        Commands::ReceiveOnchain {} => {
            serde_json::to_string_pretty(&sdk()?.receive_onchain().await?).map_err(|e| e.into())
        }
        Commands::InProgressSwap {} => {
            serde_json::to_string_pretty(&sdk()?.in_progress_swap().await?).map_err(|e| e.into())
        }
        Commands::ListRefundables {} => {
            serde_json::to_string_pretty(&sdk()?.list_refundables().await?).map_err(|e| e.into())
        }
        Commands::Refund {
            swap_address,
            to_address,
            sat_per_vbyte,
        } => {
            let res = sdk()?
                .refund(swap_address, to_address, sat_per_vbyte)
                .await?;
            Ok(format!("Refund tx: {}", res))
        }
        Commands::LnurlPay { lnurl } => match parse(&lnurl).await? {
            LnUrlPay { data: pd } => {
                let prompt = format!(
                    "Amount to pay in sats (min {} sat, max {} sat: ",
                    pd.min_sendable / 1000,
                    pd.max_sendable / 1000
                );

                let amount_sat = rl.readline(&prompt)?;
                let pay_res = sdk()?
                    .lnurl_pay(amount_sat.parse::<u64>()?, None, pd)
                    .await?;
                //show_results(pay_res);
                serde_json::to_string_pretty(&pay_res).map_err(|e| e.into())
            }
            _ => Err(anyhow!("Invalid input")),
        },
        Commands::LnurlWithdraw { lnurl } => {
            match parse(&lnurl).await? {
                LnUrlWithdraw { data: wd } => {
                    info!("Endpoint description: {}", wd.default_description);

                    // Bounds for a withdrawal amount. Normally these would also consider NodeState params:
                    // max can receive = min(maxWithdrawable, local estimation of how much can be routed into wallet)
                    // min can receive = max(minWithdrawable, local minimal value allowed by wallet)
                    // However, for simplicity, we just use the LNURL-withdraw min/max bounds
                    let user_input_max_sat = wd.max_withdrawable / 1000;
                    let user_input_min_sat = 2001;

                    if user_input_max_sat < user_input_min_sat {
                        error!("The LNURLw endpoint needs to accept at least {} sats, but min / max withdrawable are {} sat / {} sat",
                user_input_min_sat,
                wd.min_withdrawable / 1000,
                wd.max_withdrawable / 1000
            );
                        return Ok("".to_string());
                    }

                    let prompt = format!(
            "Amount to withdraw in sats (min {user_input_min_sat} sat, max {user_input_max_sat} sat: ",
        );
                    let user_input_withdraw_amount_sat = rl.readline(&prompt)?;

                    let amount_sats: u64 = user_input_withdraw_amount_sat.parse()?;
                    let description = "LNURL-withdraw";

                    let withdraw_res = sdk()?
                        .lnurl_withdraw(wd, amount_sats, Some(description.into()))
                        .await?;
                    serde_json::to_string_pretty(&withdraw_res).map_err(|e| e.into())
                }
                _ => Err(anyhow!("Unexpected result type")),
            }
        }
        Commands::LnurlAuth { lnurl } => {
            let lnurl_endpoint = lnurl.trim();

            match parse(lnurl_endpoint).await? {
                LnUrlAuth { data: ad } => {
                    println!("received {ad:?}");

                    let auth_res = sdk()?.lnurl_auth(ad).await?;
                    serde_json::to_string_pretty(&auth_res).map_err(|e| e.into())
                }
                _ => Err(anyhow!("Unexpected result type")),
            }
        }
        Commands::ExecuteDevCommand { command } => {
            serde_json::to_string_pretty(&sdk()?.execute_dev_command(command).await?)
                .map_err(|e| e.into())
        }
    }
}
