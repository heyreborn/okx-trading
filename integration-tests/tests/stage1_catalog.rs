//! Stage-one cross-package directory checks. The live demo test is ignored
//! until a separate, explicitly authorized read-only demo credential is set.

use config::business;
use instrument::{
    AccountEligibility, AccountPermit, Directory, FeeSchedule, InstrumentSpec, ProductMapping,
    TradingState,
};
use model::facts::{PositionMode, ProductKind};
use model::units::{Decimal, ReceivedTimeMs, SourceTimeMs};
use okx_client::{Credentials, InstrumentType, ReadOnlyClient, Region};

const SAMPLE: &str = include_str!("../../config/example.toml");

fn dec(value: &str) -> Decimal {
    Decimal::parse(value).expect("valid test decimal")
}

#[test]
fn offline_multi_product_catalog_and_mode_contract() {
    let parsed = business::parse(SAMPLE).expect("versioned config");
    let file = parsed.file();
    let account = &file.accounts[0];
    assert_eq!(file.products.len(), 2);
    assert_eq!(file.strategies.len(), 3);
    assert_eq!(
        parsed.validate_actual_mode(&account.account_id, PositionMode::NetMode),
        Err(business::Error::ActualModeMismatch)
    );
    parsed
        .validate_actual_mode(&account.account_id, PositionMode::LongShortMode)
        .expect("dual-side account supports configured target");

    let source = SourceTimeMs::new(100).expect("source time");
    let received = ReceivedTimeMs::new(100).expect("receipt time");
    let mut mappings = Vec::new();
    let mut specs = Vec::new();
    for product in &file.products {
        mappings.push(ProductMapping {
            product_id: product.product_id.clone(),
            okx_inst_id: product.okx_inst_id.clone(),
            kind: product.kind,
            base: product.base.clone(),
            quote: product.quote.clone(),
            settlement: product.settlement.clone(),
        });
        specs.push(InstrumentSpec {
            okx_inst_id: product.okx_inst_id.clone(),
            kind: product.kind,
            base: product.base.clone(),
            quote: product.quote.clone(),
            settlement: product.settlement.clone(),
            version: "sample-v1".into(),
            state: TradingState::Live,
            tick_size: dec("0.1"),
            lot_size: dec("1"),
            min_size: dec("1"),
            contract_value: (product.kind == ProductKind::LinearUsdtSwap).then(|| dec("0.01")),
            contract_value_currency: (product.kind == ProductKind::LinearUsdtSwap)
                .then(|| product.base.clone()),
            fee_group_id: Some("1".into()),
            source_time_ms: source,
            received_time_ms: received,
        });
    }
    let directory = Directory::new(mappings, specs).expect("consistent catalog");
    let permit = AccountPermit {
        account_id: account.account_id.clone(),
        allowed_modes: account.allowed_position_modes.clone(),
        product_ids: account.product_ids.clone(),
    };
    let mut eligibility = AccountEligibility {
        account_id: account.account_id.clone(),
        position_mode: PositionMode::LongShortMode,
        instrument_ids: file
            .products
            .iter()
            .map(|p| p.okx_inst_id.clone())
            .collect(),
        source_time_ms: source,
        received_time_ms: received,
    };
    let now = ReceivedTimeMs::new(105).expect("time");
    for product in &file.products {
        let fee = FeeSchedule {
            okx_inst_id: product.okx_inst_id.clone(),
            group_id: "1".into(),
            version: "fee-v1".into(),
            maker: dec("-0.0008"),
            taker: dec("-0.001"),
            source_time_ms: source,
            received_time_ms: received,
        };
        assert!(
            directory
                .assess(&product.product_id, &permit, &eligibility, &fee, now, 10)
                .is_ok()
        );
    }
    eligibility.source_time_ms = SourceTimeMs::new(90).expect("old fact");
    let product = &file.products[0];
    let fee = FeeSchedule {
        okx_inst_id: product.okx_inst_id.clone(),
        group_id: "1".into(),
        version: "fee-v1".into(),
        maker: dec("-0.0008"),
        taker: dec("-0.001"),
        source_time_ms: source,
        received_time_ms: received,
    };
    assert!(matches!(
        directory.assess(&product.product_id, &permit, &eligibility, &fee, now, 10),
        Err(instrument::Error::StaleEligibility)
    ));
}

fn secret(variable: &str) -> String {
    let path = std::env::var(variable).expect("explicit demo secret file variable is required");
    std::fs::read_to_string(path)
        .expect("read demo secret file")
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

#[tokio::test]
#[ignore = "requires explicit read-only OKX demo authorization and secret file references"]
async fn authorized_demo_catalog_acceptance() {
    assert_eq!(
        std::env::var("OKX_STAGE1_DEMO_READ_ONLY").as_deref(),
        Ok("1")
    );
    let region = match std::env::var("OKX_STAGE1_DEMO_REGION").as_deref() {
        Ok("global") => Region::Global,
        Ok("us") => Region::UnitedStates,
        _ => panic!("explicit demo region is required"),
    };
    let credentials = Credentials::new(
        secret("OKX_STAGE1_DEMO_KEY_FILE"),
        secret("OKX_STAGE1_DEMO_SECRET_FILE"),
        secret("OKX_STAGE1_DEMO_PASSPHRASE_FILE"),
    )
    .expect("valid demo credential references");
    let client = ReadOnlyClient::new_demo(region, credentials).expect("demo-only client");
    let parsed = business::parse(SAMPLE).expect("versioned sample config");
    let file = parsed.file();
    let account = &file.accounts[0];
    client.synchronize_clock().await.expect("demo system time");
    let actual = client
        .account_configuration()
        .await
        .expect("read account configuration");
    parsed
        .validate_actual_mode(&account.account_id, actual.value.position_mode)
        .expect("configured target mode matches demo account");

    let mut visible_ids = Vec::new();
    let mut eligibility_source = actual.source_time_ms;
    let mut eligibility_received = actual.received_time_ms;
    for kind in [InstrumentType::Spot, InstrumentType::Swap] {
        let observed = client
            .account_instruments(account.account_id.clone(), kind)
            .await
            .expect("read account instruments");
        eligibility_source = eligibility_source.min(observed.source_time_ms);
        eligibility_received = eligibility_received.max(observed.received_time_ms);
        visible_ids.extend(observed.value.inst_ids);
    }
    let eligibility = AccountEligibility {
        account_id: account.account_id.clone(),
        position_mode: actual.value.position_mode,
        instrument_ids: visible_ids,
        source_time_ms: eligibility_source,
        received_time_ms: eligibility_received,
    };
    let permit = AccountPermit {
        account_id: account.account_id.clone(),
        allowed_modes: account.allowed_position_modes.clone(),
        product_ids: account.product_ids.clone(),
    };
    let mappings: Vec<ProductMapping> = file
        .products
        .iter()
        .map(|p| ProductMapping {
            product_id: p.product_id.clone(),
            okx_inst_id: p.okx_inst_id.clone(),
            kind: p.kind,
            base: p.base.clone(),
            quote: p.quote.clone(),
            settlement: p.settlement.clone(),
        })
        .collect();
    let mut specs = Vec::new();
    let mut fees = Vec::new();
    for product in &file.products {
        let kind = match product.kind {
            ProductKind::Spot => InstrumentType::Spot,
            ProductKind::LinearUsdtSwap => InstrumentType::Swap,
        };
        let observed = client
            .public_instrument(kind, &product.okx_inst_id)
            .await
            .expect("read allowlisted public instrument");
        let group_id = observed
            .value
            .fee_group_id
            .as_deref()
            .expect("instrument fee group");
        let fee = client
            .fee_group(kind, group_id)
            .await
            .expect("read applicable fee group");
        fees.push(
            fee.to_fee_schedule(product.okx_inst_id.clone())
                .expect("typed fee fact"),
        );
        specs.push(
            observed
                .to_instrument_spec()
                .expect("typed instrument spec"),
        );
    }
    let directory = Directory::new(mappings, specs).expect("catalog mapping");
    let now = ReceivedTimeMs::new(
        i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_millis(),
        )
        .expect("time range"),
    )
    .expect("time");
    for (product, fee) in file.products.iter().zip(&fees) {
        directory
            .assess(&product.product_id, &permit, &eligibility, fee, now, 30_000)
            .expect("fresh account-authorized read-only product");
    }
}
